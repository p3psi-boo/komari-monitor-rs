use icmp_socket::packet::{IcmpPacketBuildError, WithEchoRequest};
use icmp_socket::{
    IcmpSocket, IcmpSocket4, IcmpSocket6, Icmpv4Message, Icmpv4Packet, Icmpv6Message, Icmpv6Packet,
};
use log::{debug, warn};
use miniserde::{Deserialize, Serialize};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::str::FromStr;
use std::sync::atomic::AtomicU16;
use std::time::Duration;
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;
use tokio::net::TcpStream;
use tokio::net::lookup_host;
use tokio::time::Instant;

// =============== ICMP Trait Abstractions ===============

/// Trait for ICMP socket operations (IPv4/IPv6 agnostic)
trait IcmpSocketExt: Sized {
    type Addr;
    type Packet: IcmpPacketExt + IcmpPacketMessageExt;

    fn new() -> Result<Self, String>;
    fn bind(&mut self, addr: Self::Addr) -> Result<(), String>;
    fn send_to(&mut self, addr: Self::Addr, packet: Self::Packet) -> Result<(), String>;
    fn set_timeout(&mut self, timeout: Option<Duration>);
    fn rcv_from(&mut self) -> Result<Self::Packet, String>;
}

/// Trait for ICMP packet creation
trait IcmpPacketExt: Sized {
    fn with_echo_request(
        identifier: u16,
        sequence: u16,
        payload: Vec<u8>,
    ) -> Result<Self, IcmpPacketBuildError>;
}

/// Trait for accessing the message field from ICMP packet
trait IcmpPacketMessageExt {
    fn match_echo_reply(&self, expected_id: u16) -> bool;
}

// IcmpSocket4 implementations
impl IcmpSocketExt for IcmpSocket4 {
    type Addr = Ipv4Addr;
    type Packet = Icmpv4Packet;

    fn new() -> Result<Self, String> {
        Self::new().map_err(|_| "Failed to create socket".to_string())
    }

    fn bind(&mut self, addr: Self::Addr) -> Result<(), String> {
        IcmpSocket::bind(self, addr).map_err(|_| "Failed to bind socket".to_string())
    }

    fn send_to(&mut self, addr: Self::Addr, packet: Self::Packet) -> Result<(), String> {
        IcmpSocket::send_to(self, addr, packet).map_err(|_| "Send failed".to_string())
    }

    fn set_timeout(&mut self, timeout: Option<Duration>) {
        IcmpSocket::set_timeout(self, timeout);
    }

    fn rcv_from(&mut self) -> Result<Self::Packet, String> {
        IcmpSocket::rcv_from(self)
            .map(|(packet, _)| packet)
            .map_err(|_| "Receive failed".to_string())
    }
}

// IcmpSocket6 implementations
impl IcmpSocketExt for IcmpSocket6 {
    type Addr = Ipv6Addr;
    type Packet = Icmpv6Packet;

    fn new() -> Result<Self, String> {
        Self::new().map_err(|_| "Failed to create socket".to_string())
    }

    fn bind(&mut self, addr: Self::Addr) -> Result<(), String> {
        IcmpSocket::bind(self, addr).map_err(|_| "Failed to bind socket".to_string())
    }

    fn send_to(&mut self, addr: Self::Addr, packet: Self::Packet) -> Result<(), String> {
        IcmpSocket::send_to(self, addr, packet).map_err(|_| "Send failed".to_string())
    }

    fn set_timeout(&mut self, timeout: Option<Duration>) {
        IcmpSocket::set_timeout(self, timeout);
    }

    fn rcv_from(&mut self) -> Result<Self::Packet, String> {
        IcmpSocket::rcv_from(self)
            .map(|(packet, _)| packet)
            .map_err(|_| "Receive failed".to_string())
    }
}

// Packet implementations
impl IcmpPacketExt for Icmpv4Packet {
    fn with_echo_request(
        identifier: u16,
        sequence: u16,
        payload: Vec<u8>,
    ) -> Result<Self, IcmpPacketBuildError> {
        <Self as WithEchoRequest>::with_echo_request(identifier, sequence, payload)
    }
}

impl IcmpPacketExt for Icmpv6Packet {
    fn with_echo_request(
        identifier: u16,
        sequence: u16,
        payload: Vec<u8>,
    ) -> Result<Self, IcmpPacketBuildError> {
        <Self as WithEchoRequest>::with_echo_request(identifier, sequence, payload)
    }
}

// Packet message matching implementations
impl IcmpPacketMessageExt for Icmpv4Packet {
    fn match_echo_reply(&self, expected_id: u16) -> bool {
        matches!(
            &self.message,
            Icmpv4Message::EchoReply { identifier, .. } if *identifier == expected_id
        )
    }
}

impl IcmpPacketMessageExt for Icmpv6Packet {
    fn match_echo_reply(&self, expected_id: u16) -> bool {
        matches!(
            &self.message,
            Icmpv6Message::EchoReply { identifier, .. } if *identifier == expected_id
        )
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct PingEvent {
    message: String,
    ping_task_id: u64,
    ping_type: String,
    ping_target: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct PingEventCallback {
    #[serde(rename = "type")]
    pub type_str: String,
    pub task_id: u64,
    pub ping_type: String,
    pub value: Option<i64>,
    pub finished_at: String,
}

fn build_ping_callback(task_id: u64, ping_type: &str, value: Option<i64>) -> PingEventCallback {
    let now = OffsetDateTime::now_local().unwrap_or_else(|_| OffsetDateTime::now_utc());
    let finished_at = now.format(&Rfc3339).unwrap_or_default();

    PingEventCallback {
        type_str: String::from("ping_result"),
        task_id,
        ping_type: ping_type.to_string(),
        value,
        finished_at,
    }
}

fn parse_direct_tcp_target(addr: &str) -> Option<SocketAddr> {
    if let Ok(socket_addr) = addr.parse::<SocketAddr>() {
        return Some(socket_addr);
    }

    if let Some(host) = addr.strip_prefix('[').and_then(|value| value.strip_suffix(']'))
        && let Ok(ip) = host.parse::<IpAddr>()
    {
        return Some(SocketAddr::new(ip, 80));
    }

    if let Ok(ip) = addr.parse::<IpAddr>() {
        return Some(SocketAddr::new(ip, 80));
    }

    None
}

async fn resolve_tcp_target(addr: &str) -> Result<SocketAddr, String> {
    if let Some(socket_addr) = parse_direct_tcp_target(addr) {
        return Ok(socket_addr);
    }

    if let Ok(mut addrs) = lookup_host(addr).await
        && let Some(addr) = addrs.next()
    {
        return Ok(addr);
    }

    let with_default_port = format!("{addr}:80");
    let mut addrs = lookup_host(&with_default_port)
        .await
        .map_err(|e| format!("Invalid TCP target `{addr}`: {e}"))?;

    addrs
        .next()
        .ok_or_else(|| format!("No socket addresses found for `{addr}`"))
}

fn ping_http_blocking(target: &str) -> bool {
    #[cfg(feature = "ureq-support")]
    {
        return ureq::get(target)
            .header("User-Agent", "curl/11.45.14")
            .call()
            .is_ok();
    }

    #[cfg(all(not(feature = "ureq-support"), feature = "nyquest-support"))]
    {
        use nyquest::Request;
        let client = crate::utils::create_nyquest_client(false);
        let request = Request::get(target);
        return client.request(request).is_ok();
    }

    #[allow(unreachable_code)]
    false
}

pub async fn ping_target(utf8_str: &str) -> Result<PingEventCallback, String> {
    let ping_event: PingEvent =
        miniserde::json::from_str(utf8_str).map_err(|_| "Failed to parse PingEvent".to_string())?;

    match ping_event.ping_type.as_str() {
        "icmp" => match get_ip_from_string(&ping_event.ping_target).await {
            Ok(ip) => {
                debug!("DNS resolution: {}: {}", ping_event.ping_target, ip);
                let task_id = ping_event.ping_task_id;
                let result = tokio::task::spawn_blocking(move || match ip {
                    IpAddr::V4(ip) => icmp_ipv4(ip, task_id),
                    IpAddr::V6(ip) => icmp_ipv6(ip, task_id),
                })
                .await
                .map_err(|e| format!("Failed to join ICMP task: {e}"))?;
                result
            }
            Err(e) => {
                warn!("DNS resolution failed: {}: {}", ping_event.ping_target, e);
                Err(String::from("Failed to resolve IP address"))
            }
        },
        "tcp" => {
            let start_time = Instant::now();
            let target = resolve_tcp_target(&ping_event.ping_target).await?;

            let ping = match tokio::time::timeout(Duration::from_secs(10), TcpStream::connect(target)).await {
                Err(_) => Err("Tcping timeout".to_string()),
                Ok(Ok(_)) => Ok(()),
                Ok(Err(_)) => Err("Failed to connect".to_string()),
            };

            let rtt = i64::try_from(start_time.elapsed().as_millis()).ok();
            if ping.is_ok() {
                Ok(build_ping_callback(ping_event.ping_task_id, "tcp", rtt))
            } else {
                Ok(build_ping_callback(ping_event.ping_task_id, "tcp", Some(-1)))
            }
        }
        "http" => {
            let start_time = Instant::now();
            let target = ping_event.ping_target.clone();
            let result = tokio::task::spawn_blocking(move || ping_http_blocking(&target))
                .await
                .map_err(|e| format!("Failed to join HTTP ping task: {e}"))?;

            if result {
                Ok(build_ping_callback(
                    ping_event.ping_task_id,
                    "http",
                    i64::try_from(start_time.elapsed().as_millis()).ok(),
                ))
            } else {
                Ok(build_ping_callback(ping_event.ping_task_id, "http", Some(-1)))
            }
        }
        _ => Err(format!("Ping Error: Not Support: {}", ping_event.ping_type)),
    }
}

pub async fn get_ip_from_string(host_or_ip: &str) -> Result<IpAddr, String> {
    if let Ok(ip) = IpAddr::from_str(host_or_ip) {
        return Ok(ip);
    }

    let host_with_port = format!("{host_or_ip}:80");
    match lookup_host(&host_with_port).await {
        Ok(mut ip_addresses) => {
            if let Some(first_socket_addr) = ip_addresses.next() {
                Ok(first_socket_addr.ip())
            } else {
                Err(format!(
                    "No IP addresses found for the domain: {host_or_ip}"
                ))
            }
        }
        Err(e) => Err(format!("Error looking up domain: {e}")),
    }
}

/// Generic ICMP ping implementation for both IPv4 and IPv6
fn icmp_ping_generic<S>(ip: S::Addr, task_id: u64, bind_addr: S::Addr) -> Result<PingEventCallback, String>
where
    S: IcmpSocketExt,
{
    let mut socket = S::new()?;

    if socket.bind(bind_addr).is_err() {
        return Err(String::from("Failed to bind Raw socket"));
    }

    let identifier = get_identifier();
    let packet = S::Packet::with_echo_request(
        identifier,
        0,
        vec![
            0x20, 0x20, 0x75, 0x73, 0x74, 0x20, 0x61, 0x20, 0x66, 0x6c, 0x65, 0x73, 0x68, 0x20,
            0x77, 0x6f, 0x75, 0x6e, 0x64, 0x20, 0x20, 0x74, 0x69, 0x73, 0x20, 0x62, 0x75, 0x74,
            0x20, 0x61, 0x20, 0x73, 0x63, 0x72, 0x61, 0x74, 0x63, 0x68, 0x20, 0x20, 0x6b, 0x6e,
            0x69, 0x67, 0x68, 0x74, 0x73, 0x20, 0x6f, 0x66, 0x20, 0x6e, 0x69, 0x20, 0x20, 0x20,
        ],
    )
    .map_err(|_| "Failed to create ICMP packet".to_string())?;

    let timeout = Duration::from_secs(3);
    let send_time = Instant::now();
    if socket.send_to(ip, packet).is_err() {
        return Ok(build_ping_callback(task_id, "icmp", Some(-1)));
    }

    loop {
        let new_timeout = timeout.checked_sub(send_time.elapsed());
        if new_timeout.is_none() {
            break;
        }
        socket.set_timeout(new_timeout);

        let Ok(resp) = socket.rcv_from() else {
            break;
        };

        if resp.match_echo_reply(identifier) {
            let rtt = send_time.elapsed();
            return Ok(build_ping_callback(
                task_id,
                "icmp",
                i64::try_from(rtt.as_millis()).ok(),
            ));
        }
    }

    Ok(build_ping_callback(task_id, "icmp", Some(-1)))
}

pub fn icmp_ipv4(ip: Ipv4Addr, task_id: u64) -> Result<PingEventCallback, String> {
    icmp_ping_generic::<IcmpSocket4>(ip, task_id, Ipv4Addr::new(0, 0, 0, 0))
}

pub fn icmp_ipv6(ip: Ipv6Addr, task_id: u64) -> Result<PingEventCallback, String> {
    icmp_ping_generic::<IcmpSocket6>(ip, task_id, Ipv6Addr::new(0, 0, 0, 0, 0, 0, 0, 0))
}

fn get_identifier() -> u16 {
    static GENERATOR: std::sync::LazyLock<AtomicU16> = std::sync::LazyLock::new(|| {
        AtomicU16::new(std::process::id() as u16 ^ OffsetDateTime::now_utc().millisecond())
    });

    GENERATOR.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}
