use crate::command_parser::LogLevel;
use crate::rustls_config::create_dangerous_config;
use log::Level;
use std::fmt::Display;
use std::sync::Arc;
use std::time::Duration;
use tokio::net::TcpStream;
use tokio::time::timeout;
use tokio_tungstenite::{
    Connector, MaybeTlsStream, WebSocketStream, connect_async, connect_async_tls_with_config,
};
use url::Url;

pub fn init_logger(log_level: &LogLevel) {
    #[cfg(target_os = "windows")]
    simple_logger::set_up_windows_color_terminal();

    match log_level {
        LogLevel::Error => simple_logger::init_with_level(Level::Error).unwrap(),
        LogLevel::Warn => simple_logger::init_with_level(Level::Warn).unwrap(),
        LogLevel::Info => simple_logger::init_with_level(Level::Info).unwrap(),
        LogLevel::Debug => simple_logger::init_with_level(Level::Debug).unwrap(),
        LogLevel::Trace => simple_logger::init_with_level(Level::Trace).unwrap(),
    }
}

#[derive(Debug, Clone)]
pub struct ConnectionUrls {
    pub basic_info: String,
    pub exec_callback: String,
    pub ws_terminal: String,
    pub ws_real_time: String,
}

impl Display for ConnectionUrls {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "Connection URLs:")?;
        writeln!(f, "  Basic Info URL: {}", mask_url_token(&self.basic_info))?;
        writeln!(f, "  Exec Callback URL: {}", mask_url_token(&self.exec_callback))?;
        writeln!(
            f,
            "  WebSocket Terminal URL: {}",
            mask_url_token(&self.ws_terminal)
        )?;
        writeln!(
            f,
            "  WebSocket Real-time URL: {}",
            mask_url_token(&self.ws_real_time)
        )
    }
}

fn mask_url_token(raw_url: &str) -> String {
    let Ok(mut parsed) = Url::parse(raw_url) else {
        return raw_url.to_string();
    };

    let pairs: Vec<(String, String)> = parsed
        .query_pairs()
        .map(|(k, v)| {
            if k == "token" {
                (k.to_string(), "***".to_string())
            } else {
                (k.to_string(), v.to_string())
            }
        })
        .collect();

    parsed.query_pairs_mut().clear();
    for (key, value) in pairs {
        parsed.query_pairs_mut().append_pair(&key, &value);
    }

    parsed.to_string()
}

fn build_url(base: &Url, path: &str, token: &str) -> String {
    let mut url = base.clone();
    url.set_path(path);
    url.query_pairs_mut().clear().append_pair("token", token);
    url.to_string()
}

pub fn build_urls(
    http_server: &str,
    ws_server: Option<&String>,
    token: &str,
) -> Result<ConnectionUrls, String> {
    let http_url = Url::parse(http_server).map_err(|e| format!("Invalid http server URL: {e}"))?;

    let ws_url = if let Some(ws) = ws_server {
        Url::parse(ws).map_err(|e| format!("Invalid websocket server URL: {e}"))?
    } else {
        let mut ws_url = http_url.clone();
        match ws_url.scheme() {
            "http" => ws_url
                .set_scheme("ws")
                .map_err(|_| "Failed to derive ws scheme from http".to_string())?,
            "https" => ws_url
                .set_scheme("wss")
                .map_err(|_| "Failed to derive wss scheme from https".to_string())?,
            other => return Err(format!("Unsupported scheme for http server: {other}")),
        }
        ws_url
    };

    Ok(ConnectionUrls {
        basic_info: build_url(&http_url, "/api/clients/uploadBasicInfo", token),
        exec_callback: build_url(&http_url, "/api/clients/task/result", token),
        ws_terminal: build_url(&ws_url, "/api/clients/terminal", token),
        ws_real_time: build_url(&ws_url, "/api/clients/report", token),
    })
}

pub async fn connect_ws(
    url: &str,
    skip_verify: bool,
) -> Result<WebSocketStream<MaybeTlsStream<TcpStream>>, String> {
    let parsed = Url::parse(url).map_err(|e| format!("Invalid WebSocket URL `{url}`: {e}"))?;
    let connection_timeout = Duration::from_secs(10);

    match parsed.scheme() {
        "wss" => {
            if skip_verify {
                timeout(
                    connection_timeout,
                    connect_async_tls_with_config(
                        url,
                        None,
                        false,
                        Some(Connector::Rustls(Arc::new(create_dangerous_config()))),
                    ),
                )
                .await
                .map_err(|_| format!("WebSocket connection timeout after {connection_timeout:?}"))?
                .map(|ws| ws.0)
                .map_err(|e| format!("Failed to establish secure WebSocket connection: {e}"))
            } else {
                timeout(
                    connection_timeout,
                    connect_async_tls_with_config(url, None, false, None),
                )
                .await
                .map_err(|_| format!("WebSocket connection timeout after {connection_timeout:?}"))?
                .map(|ws| ws.0)
                .map_err(|e| format!("Failed to establish secure WebSocket connection: {e}"))
            }
        }
        "ws" => timeout(connection_timeout, connect_async(url))
            .await
            .map_err(|_| format!("WebSocket connection timeout after {connection_timeout:?}"))?
            .map(|ws| ws.0)
            .map_err(|e| format!("Failed to establish WebSocket connection: {e}")),
        other => Err(format!(
            "Unsupported WebSocket URL scheme `{other}`, expected `ws` or `wss`"
        )),
    }
}

#[cfg(feature = "ureq-support")]
pub fn create_ureq_agent(disable_verification: bool) -> ureq::Agent {
    let config = ureq::Agent::config_builder()
        .tls_config(
            ureq::tls::TlsConfig::builder()
                .disable_verification(disable_verification)
                .build(),
        )
        .timeout_global(Some(Duration::from_secs(5)))
        .build();
    config.new_agent()
}

#[cfg(all(not(feature = "ureq-support"), feature = "nyquest-support"))]
pub fn create_nyquest_client(disable_verification: bool) -> nyquest::BlockingClient {
    use std::time::Duration;
    let mut client = nyquest::ClientBuilder::default()
        .request_timeout(Duration::from_secs(5))
        .user_agent("curl/8.7.1");
    if disable_verification {
        client = client.dangerously_ignore_certificate_errors();
    }
    client.build_blocking().unwrap()
}
