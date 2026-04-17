use crate::command_parser::IpProvider;
use log::trace;
use miniserde::{Deserialize, Serialize, json};
use std::net::{Ipv4Addr, Ipv6Addr};
use std::str::FromStr;
use tokio::task::JoinHandle;

#[cfg(feature = "ureq-support")]
use std::time::Duration;

pub async fn ip(provider: &IpProvider) -> IPInfo {
    match provider {
        IpProvider::Cloudflare => ip_cloudflare().await,
        IpProvider::Ipinfo => ip_ipinfo().await,
    }
}

#[derive(Debug)]
pub struct IPInfo {
    pub ipv4: Option<Ipv4Addr>,
    pub ipv6: Option<Ipv6Addr>,
}

#[derive(Serialize, Deserialize)]
struct IpJson {
    ip: String,
}

enum IpFamily {
    V4Only,
    V6Only,
}

fn fetch_url_blocking(url: &str, family: Option<IpFamily>) -> Option<String> {
    #[cfg(not(feature = "ureq-support"))]
    let _ = family;

    #[cfg(feature = "ureq-support")]
    {
        let mut request = ureq::get(url)
            .header("User-Agent", "curl/8.7.1")
            .config()
            .timeout_global(Some(Duration::from_secs(5)));

        if let Some(family) = family {
            request = request.ip_family(match family {
                IpFamily::V4Only => ureq::config::IpFamily::Ipv4Only,
                IpFamily::V6Only => ureq::config::IpFamily::Ipv6Only,
            });
        }

        let response = request.build().call();
        if let Ok(mut response) = response {
            return response.body_mut().read_to_string().ok();
        }
        return None;
    }

    #[cfg(all(not(feature = "ureq-support"), feature = "nyquest-support"))]
    {
        use nyquest::Request;
        let client = crate::utils::create_nyquest_client(false);
        let request = Request::get(url);
        if let Ok(response) = client.request(request) {
            return response.text().ok();
        }
        return None;
    }

    #[allow(unreachable_code)]
    None
}

async fn fetch_ipinfo_v4() -> Option<String> {
    tokio::task::spawn_blocking(|| fetch_url_blocking("https://ipinfo.io", Some(IpFamily::V4Only)))
        .await
        .ok()
        .flatten()
}

async fn fetch_ipinfo_v6() -> Option<String> {
    tokio::task::spawn_blocking(|| fetch_url_blocking("https://6.ipinfo.io", Some(IpFamily::V6Only)))
        .await
        .ok()
        .flatten()
}

async fn fetch_cloudflare_v4() -> Option<String> {
    tokio::task::spawn_blocking(|| {
        fetch_url_blocking("https://www.cloudflare.com/cdn-cgi/trace", Some(IpFamily::V4Only))
    })
    .await
    .ok()
    .flatten()
}

async fn fetch_cloudflare_v6() -> Option<String> {
    tokio::task::spawn_blocking(|| {
        fetch_url_blocking("https://www.cloudflare.com/cdn-cgi/trace", Some(IpFamily::V6Only))
    })
    .await
    .ok()
    .flatten()
}

fn parse_ipinfo_response(body: &str) -> Option<String> {
    let json: IpJson = json::from_str(body).ok()?;
    Some(json.ip)
}

fn extract_cloudflare_ip(body: &str) -> Option<String> {
    for line in body.lines() {
        if line.starts_with("ip=") {
            return Some(line.replace("ip=", ""));
        }
    }
    None
}

pub async fn ip_ipinfo() -> IPInfo {
    let ipv4: JoinHandle<Option<Ipv4Addr>> = tokio::spawn(async move {
        if let Some(body) = fetch_ipinfo_v4().await {
            if let Some(ip_str) = parse_ipinfo_response(&body) {
                return Ipv4Addr::from_str(&ip_str).ok();
            }
        }
        None
    });

    let ipv6: JoinHandle<Option<Ipv6Addr>> = tokio::spawn(async move {
        if let Some(body) = fetch_ipinfo_v6().await {
            if let Some(ip_str) = parse_ipinfo_response(&body) {
                return Ipv6Addr::from_str(&ip_str).ok();
            }
        }
        None
    });

    let ipv4_result = ipv4.await.unwrap_or(None);
    let ipv6_result = ipv6.await.unwrap_or(None);

    let ip_info = IPInfo {
        ipv4: ipv4_result,
        ipv6: ipv6_result,
    };

    trace!("IP INFO (ipinfo) successfully retrieved: {:?}", ip_info);

    ip_info
}

pub async fn ip_cloudflare() -> IPInfo {
    let ipv4: JoinHandle<Option<Ipv4Addr>> = tokio::spawn(async move {
        if let Some(body) = fetch_cloudflare_v4().await {
            if let Some(ip_str) = extract_cloudflare_ip(&body) {
                return Ipv4Addr::from_str(&ip_str).ok();
            }
        }
        None
    });

    let ipv6: JoinHandle<Option<Ipv6Addr>> = tokio::spawn(async move {
        if let Some(body) = fetch_cloudflare_v6().await {
            if let Some(ip_str) = extract_cloudflare_ip(&body) {
                return Ipv6Addr::from_str(&ip_str).ok();
            }
        }
        None
    });

    let ipv4_result = ipv4.await.unwrap_or(None);
    let ipv6_result = ipv6.await.unwrap_or(None);

    let ip_info = IPInfo {
        ipv4: ipv4_result,
        ipv6: ipv6_result,
    };

    trace!("IP INFO (cloudflare) successfully retrieved: {:?}", ip_info);

    ip_info
}
