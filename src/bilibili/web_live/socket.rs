use futures_util::{SinkExt, StreamExt};
use serde::Serialize;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite;

use super::auth::WebLiveAuth;
use super::parser::{
    build_packet, collect_commands, parse_packets, OP_AUTH, OP_CONNECT_SUCCESS, OP_HEARTBEAT,
    OP_HEARTBEAT_REPLY, OP_MESSAGE,
};

const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(20);

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type")]
pub enum SocketStatus {
    #[serde(rename = "disconnected")]
    Disconnected {
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<String>,
    },
    #[serde(rename = "connecting")]
    Connecting {},
    #[serde(rename = "connected")]
    Connected {},
}

pub struct SocketHandle {
    pub status_rx: tokio::sync::watch::Receiver<SocketStatus>,
    pub(crate) cancel: tokio_util::sync::CancellationToken,
    pub(crate) abort_handle: tokio::task::AbortHandle,
}

impl SocketHandle {
    pub fn stop(&self) {
        self.cancel.cancel();
        self.abort_handle.abort();
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancel.is_cancelled()
    }
}

pub fn connect(auth: WebLiveAuth) -> (SocketHandle, mpsc::Receiver<serde_json::Value>) {
    let (status_tx, status_rx) =
        tokio::sync::watch::channel(SocketStatus::Disconnected { error: None });
    let (command_tx, command_rx) = mpsc::channel(256);
    let cancel = tokio_util::sync::CancellationToken::new();

    let task = tokio::spawn(run_connection(auth, status_tx, command_tx, cancel.clone()));
    let abort_handle = task.abort_handle();

    let handle = SocketHandle {
        status_rx,
        cancel,
        abort_handle,
    };

    (handle, command_rx)
}

async fn run_connection(
    auth: WebLiveAuth,
    status_tx: tokio::sync::watch::Sender<SocketStatus>,
    command_tx: mpsc::Sender<serde_json::Value>,
    cancel: tokio_util::sync::CancellationToken,
) {
    let _ = status_tx.send(SocketStatus::Connecting {});

    let url = match auth.urls.first() {
        Some(u) => u.clone(),
        None => {
            let _ = status_tx.send(SocketStatus::Disconnected {
                error: Some("no WebSocket URLs available".into()),
            });
            return;
        }
    };

    let ws_result = tokio_tungstenite::connect_async_tls_with_config(&url, None, false, None).await;

    let ws_stream = match ws_result {
        Ok((stream, _)) => {
            tracing::info!("WebSocket connected to {url}");
            stream
        }
        Err(e) => {
            tracing::warn!("WebSocket connect failed: {e}");
            let _ = status_tx.send(SocketStatus::Disconnected {
                error: Some(format!("connect error: {e}")),
            });
            return;
        }
    };

    if cancel.is_cancelled() {
        let _ = status_tx.send(SocketStatus::Disconnected { error: None });
        return;
    }

    let (mut sink, mut stream) = ws_stream.split();

    let auth_body = serde_json::json!({
        "uid": auth.uid.unwrap_or(0),
        "roomid": auth.room_id,
        "protover": 3,
        "platform": "web",
        "type": 2,
        "key": auth.key,
        "buvid": auth.buvid3,
    });
    let auth_json = match serde_json::to_string(&auth_body) {
        Ok(s) => s,
        Err(e) => {
            let _ = status_tx.send(SocketStatus::Disconnected {
                error: Some(format!("auth serialize error: {e}")),
            });
            return;
        }
    };
    let auth_packet = build_packet(OP_AUTH, &auth_json);

    if sink
        .send(tungstenite::Message::Binary(auth_packet.into()))
        .await
        .is_err()
    {
        let _ = status_tx.send(SocketStatus::Disconnected {
            error: Some("failed to send auth packet".into()),
        });
        return;
    }

    let mut heartbeat_interval = tokio::time::interval(HEARTBEAT_INTERVAL);
    heartbeat_interval.tick().await;

    loop {
        tokio::select! {
            msg = stream.next() => {
                match msg {
                    Some(Ok(tungstenite::Message::Binary(data))) => {
                        handle_data(&data, &status_tx, &command_tx).await;
                    }
                    Some(Ok(tungstenite::Message::Close(_))) => {
                        tracing::info!("connection closed by server");
                        let _ = status_tx.send(SocketStatus::Disconnected {
                            error: Some("connection closed by server".into()),
                        });
                        return;
                    }
                    Some(Ok(_)) => {}
                    Some(Err(e)) => {
                        tracing::warn!("read error: {e}");
                        let _ = status_tx.send(SocketStatus::Disconnected {
                            error: Some(format!("read error: {e}")),
                        });
                        return;
                    }
                    None => {
                        let _ = status_tx.send(SocketStatus::Disconnected {
                            error: Some("stream ended".into()),
                        });
                        return;
                    }
                }
            }
            _ = heartbeat_interval.tick() => {
                let hb = build_packet(OP_HEARTBEAT, "");
                if sink.send(tungstenite::Message::Binary(hb.into())).await.is_err() {
                    let _ = status_tx.send(SocketStatus::Disconnected {
                        error: Some("heartbeat send failed".into()),
                    });
                    return;
                }
            }
            _ = cancel.cancelled() => {
                tracing::info!("connection cancelled");
                let _ = sink.close().await;
                let _ = status_tx.send(SocketStatus::Disconnected { error: None });
                return;
            }
        }
    }
}

async fn handle_data(
    data: &[u8],
    status_tx: &tokio::sync::watch::Sender<SocketStatus>,
    command_tx: &mpsc::Sender<serde_json::Value>,
) {
    for packet in parse_packets(data) {
        let op = packet.op;
        match op {
            _ if op == OP_CONNECT_SUCCESS => {
                tracing::info!("auth success");
                let _ = status_tx.send(SocketStatus::Connected {});
            }
            _ if op == OP_HEARTBEAT_REPLY => {}
            _ if op == OP_MESSAGE => {
                let commands = collect_commands(packet.protover, &packet.body);
                for cmd in commands {
                    let _ = command_tx.send(cmd).await;
                }
            }
            _ => {
                tracing::debug!("unknown op: {op}");
            }
        }
    }
}
