use axum::extract::ws::{Message, WebSocket};
use axum::extract::{State, WebSocketUpgrade};
use axum::response::IntoResponse;
use tokio::sync::broadcast;

use super::state::{AppState, SharedState};

pub async fn panel(ws: WebSocketUpgrade, State(state): State<AppState>) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_panel(socket, state.shared))
}

pub async fn overlay(ws: WebSocketUpgrade, State(state): State<AppState>) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_overlay(socket, state.shared))
}

async fn handle_panel(mut socket: WebSocket, state: SharedState) {
    let mut rx = state.panel_tx.subscribe();
    loop {
        tokio::select! {
            result = rx.recv() => {
                match result {
                    Ok(event) => {
                        let json = match serde_json::to_string(&event) {
                            Ok(s) => s,
                            Err(e) => {
                                tracing::error!("failed to serialize panel event: {e}");
                                continue;
                            }
                        };
                        if socket.send(Message::Text(json.into())).await.is_err() {
                            break;
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(_) => break,
                }
            }
            result = socket.recv() => {
                match result {
                    Some(Ok(_)) => {}
                    _ => break,
                }
            }
        }
    }
}

async fn handle_overlay(mut socket: WebSocket, state: SharedState) {
    let mut rx = state.overlay_tx.subscribe();
    loop {
        tokio::select! {
            result = rx.recv() => {
                match result {
                    Ok(event) => {
                        let json = match serde_json::to_string(&event) {
                            Ok(s) => s,
                            Err(e) => {
                                tracing::error!("failed to serialize overlay event: {e}");
                                continue;
                            }
                        };
                        if socket.send(Message::Text(json.into())).await.is_err() {
                            break;
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(_) => break,
                }
            }
            result = socket.recv() => {
                match result {
                    Some(Ok(_)) => {}
                    _ => break,
                }
            }
        }
    }
}
