use axum::extract::ws::{Message, WebSocket};
use axum::extract::{State, WebSocketUpgrade};
use axum::response::IntoResponse;
use tokio::sync::broadcast;

use super::state::SharedState;

pub async fn panel(ws: WebSocketUpgrade, State(state): State<SharedState>) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_panel(socket, state))
}

pub async fn overlay(ws: WebSocketUpgrade, State(state): State<SharedState>) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_overlay(socket, state))
}

async fn handle_panel(mut socket: WebSocket, state: SharedState) {
    let mut rx = state.panel_tx.subscribe();
    loop {
        tokio::select! {
            result = rx.recv() => {
                match result {
                    Ok(msg) => {
                        if socket.send(Message::Text(msg.into())).await.is_err() {
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
                    Ok(msg) => {
                        if socket.send(Message::Text(msg.into())).await.is_err() {
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
