use crate::callbacks::exec::exec_command;
use crate::callbacks::ping::ping_target;
use crate::callbacks::pty::{get_pty_ws_link, handle_pty_session};
use crate::command_parser::Args;
use crate::utils::{ConnectionUrls, connect_ws};
use futures::stream::{SplitSink, SplitStream};
use futures::{SinkExt, StreamExt};
use log::{error, info};
use miniserde::{Deserialize, Serialize, json};
use std::sync::Arc;
use tokio::net::TcpStream;
use tokio::sync::{Mutex, Semaphore};
use tokio_tungstenite::tungstenite::{Message, Utf8Bytes};
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};

pub mod exec;
pub mod ping;
pub mod pty;

#[derive(Serialize, Deserialize)]
struct Msg {
    message: String,
}

type Reader = SplitStream<WebSocketStream<MaybeTlsStream<TcpStream>>>;
type LockedWriter = Arc<Mutex<SplitSink<WebSocketStream<MaybeTlsStream<TcpStream>>, Message>>>;

pub async fn handle_callbacks(
    args: &Args,
    connection_urls: &ConnectionUrls,
    reader: &mut Reader,
    locked_writer: &LockedWriter,
) {
    let limiter = Arc::new(Semaphore::new(32));

    while let Some(msg) = reader.next().await {
        let msg = match msg {
            Ok(msg) => msg,
            Err(e) => {
                error!("Callback stream read error, reconnect required: {e}");
                break;
            }
        };

        let Ok(utf8) = msg.into_text() else {
            continue;
        };

        let json: Msg = if let Ok(value) = json::from_str(utf8.as_str()) {
            value
        } else {
            continue;
        };

        info!("Received callback message type: {}", json.message);

        let utf8_cloned = utf8.clone();

        match json.message.as_str() {
            "exec" => {
                if args.terminal {
                    let permit = match limiter.clone().acquire_owned().await {
                        Ok(permit) => permit,
                        Err(e) => {
                            error!("Callback limiter closed: {e}");
                            break;
                        }
                    };

                    tokio::spawn({
                        let utf8_cloned_for_exec = utf8_cloned.clone();
                        let exec_callback_url = connection_urls.exec_callback.clone();
                        let terminal_entry = args.terminal_entry.clone();
                        let ignore_unsafe_cert = args.ignore_unsafe_cert;

                        async move {
                            // Keep permit for task lifetime to cap concurrent callback execution.
                            let _permit = permit;
                            if let Err(e) = exec_command(
                                &utf8_cloned_for_exec,
                                exec_callback_url,
                                &terminal_entry,
                                ignore_unsafe_cert,
                            )
                            .await
                            {
                                error!("Exec Error: {e}");
                            }
                        }
                    });
                } else {
                    error!("Terminal feature is disabled");
                }
            }

            "ping" => {
                let permit = match limiter.clone().acquire_owned().await {
                    Ok(permit) => permit,
                    Err(e) => {
                        error!("Callback limiter closed: {e}");
                        break;
                    }
                };

                let locked_write_for_ping = locked_writer.clone();
                tokio::spawn(async move {
                    // Keep permit for task lifetime to cap concurrent callback execution.
                    let _permit = permit;
                    match ping_target(&utf8_cloned).await {
                        Ok(json_res) => {
                            let mut write = locked_write_for_ping.lock().await;
                            info!("Ping successful: {}", json::to_string(&json_res));
                            if let Err(e) = write
                                .send(Message::Text(Utf8Bytes::from(json::to_string(&json_res))))
                                .await
                            {
                                error!(
                                    "Error occurred while pushing ping result, attempting to reconnect: {e}"
                                );
                            }
                        }
                        Err(err) => {
                            error!("Ping Error: {err}");
                        }
                    }
                });
            }

            "terminal" => {
                if args.terminal {
                    let permit = match limiter.clone().acquire_owned().await {
                        Ok(permit) => permit,
                        Err(e) => {
                            error!("Callback limiter closed: {e}");
                            break;
                        }
                    };

                    let ws_terminal_url = connection_urls.ws_terminal.clone();
                    let args = args.clone();
                    let utf8_cloned = utf8_cloned.clone();

                    tokio::spawn(async move {
                        // Keep permit for task lifetime to cap concurrent callback execution.
                        let _permit = permit;

                        let ws_url = match get_pty_ws_link(&utf8_cloned, &ws_terminal_url) {
                            Ok(ws_url) => ws_url,
                            Err(e) => {
                                error!("Failed to get PTY WebSocket URL: {e}");
                                return;
                            }
                        };

                        let ws_stream = match connect_ws(&ws_url, args.ignore_unsafe_cert).await {
                            Ok(ws_stream) => ws_stream,
                            Err(e) => {
                                error!("Failed to connect to PTY WebSocket: {e}");
                                return;
                            }
                        };

                        if let Err(e) = handle_pty_session(ws_stream, &args.terminal_entry).await {
                            error!("PTY WebSocket handling error: {e}");
                        }
                    });
                } else {
                    error!("Terminal feature is disabled");
                }
            }
            _ => {}
        }
    }

    // Closing the writer forces the main loop to reconnect after callback stream failure.
    let mut write = locked_writer.lock().await;
    let _ = write.close().await;
}
