use bilive_chat::config::{Config, LoginState};
use bilive_chat::overlay::{server, state};
use futures_util::StreamExt;
use std::sync::{Arc, Mutex};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio_tungstenite::connect_async;

async fn spawn_server() -> (String, u16) {
    let shared = state::new();
    state::spawn_synthetic_messages(shared.clone());

    let config = Arc::new(Mutex::new(Config::default()));
    let login = Arc::new(Mutex::new(LoginState::default()));

    let router = server::build_router(shared, config, login);
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();

    tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });

    (format!("http://127.0.0.1:{port}"), port)
}

async fn http_request(port: u16, method: &str, path: &str, body: &str) -> (u16, String) {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).await.unwrap();
    let extra_headers = if body.is_empty() {
        String::new()
    } else {
        format!(
            "Content-Length: {}\r\nContent-Type: application/json\r\n",
            body.len()
        )
    };
    let request = format!(
        "{method} {path} HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n{extra_headers}\r\n{body}"
    );
    stream.write_all(request.as_bytes()).await.unwrap();

    let mut response = Vec::new();
    stream.read_to_end(&mut response).await.unwrap();
    let response = String::from_utf8_lossy(&response);

    let status_line = response.lines().next().unwrap();
    let status: u16 = status_line
        .split_whitespace()
        .nth(1)
        .unwrap()
        .parse()
        .unwrap();

    (status, response.to_string())
}

fn response_body(response: &str) -> &str {
    response
        .find("\r\n\r\n")
        .map(|i| &response[i + 4..])
        .unwrap_or("")
}

#[tokio::test]
async fn panel_page_returns_200() {
    let (_base, port) = spawn_server().await;
    let (status, body) = http_request(port, "GET", "/", "").await;
    assert_eq!(status, 200);
    assert!(body.contains("bilive-chat"));
}

#[tokio::test]
async fn overlay_page_returns_200() {
    let (_base, port) = spawn_server().await;
    let (status, body) = http_request(port, "GET", "/overlay", "").await;
    assert_eq!(status, 200);
    assert!(body.contains("chat-container"));
}

#[tokio::test]
async fn ws_panel_accepts_client() {
    let (base, _port) = spawn_server().await;
    let ws_url = base.replace("http://", "ws://") + "/ws/panel";
    let (mut ws, _) = connect_async(&ws_url).await.unwrap();

    let msg = tokio::time::timeout(std::time::Duration::from_secs(8), ws.next())
        .await
        .expect("timeout waiting for panel message")
        .unwrap()
        .unwrap();

    let text = match msg {
        tokio_tungstenite::tungstenite::Message::Text(t) => t,
        other => panic!("expected text, got {other:?}"),
    };

    let parsed: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert_eq!(parsed["type"], "status");
    assert!(parsed["message"].as_str().unwrap().contains("waiting"));
}

#[tokio::test]
async fn ws_overlay_accepts_client() {
    let (base, _port) = spawn_server().await;
    let ws_url = base.replace("http://", "ws://") + "/ws/overlay";
    let (mut ws, _) = connect_async(&ws_url).await.unwrap();

    let msg = tokio::time::timeout(std::time::Duration::from_secs(5), ws.next())
        .await
        .expect("timeout waiting for overlay message")
        .unwrap()
        .unwrap();

    let text = match msg {
        tokio_tungstenite::tungstenite::Message::Text(t) => t,
        other => panic!("expected text, got {other:?}"),
    };

    let parsed: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert_eq!(parsed["type"], "display");
    assert!(parsed["text"].as_str().unwrap().contains("system event"));
}

#[tokio::test]
async fn api_config_get_returns_defaults() {
    let (_base, port) = spawn_server().await;
    let (status, resp) = http_request(port, "GET", "/api/config", "").await;
    assert_eq!(status, 200);
    let body = response_body(&resp);
    let cfg: serde_json::Value = serde_json::from_str(body).unwrap();
    assert_eq!(cfg["host"], "127.0.0.1");
    assert_eq!(cfg["port"], 7792);
    assert_eq!(cfg["room_id"], 0);
}

#[tokio::test]
async fn api_config_post_roundtrip() {
    let (_base, port) = spawn_server().await;
    let new_config = serde_json::json!({
        "host": "0.0.0.0",
        "port": 8080,
        "room_id": 12345,
        "overlay": {},
        "filter": {}
    });
    let (status, _) = http_request(port, "POST", "/api/config", &new_config.to_string()).await;
    assert_eq!(status, 200);

    let (status, resp) = http_request(port, "GET", "/api/config", "").await;
    assert_eq!(status, 200);
    let body = response_body(&resp);
    let cfg: serde_json::Value = serde_json::from_str(body).unwrap();
    assert_eq!(cfg["host"], "0.0.0.0");
    assert_eq!(cfg["port"], 8080);
    assert_eq!(cfg["room_id"], 12345);
}

#[tokio::test]
async fn api_login_state_save_and_delete() {
    let (_base, port) = spawn_server().await;

    // save cookie
    let body = serde_json::json!({"cookie": "test_cookie_value"}).to_string();
    let (status, _) = http_request(port, "POST", "/api/bilibili/login-state", &body).await;
    assert_eq!(status, 200);

    // delete cookie
    let (status, _) = http_request(port, "DELETE", "/api/bilibili/login-state", "").await;
    assert_eq!(status, 200);
}

#[tokio::test]
async fn api_login_state_post_missing_cookie() {
    let (_base, port) = spawn_server().await;
    let (status, _) = http_request(port, "POST", "/api/bilibili/login-state", "{}").await;
    assert_eq!(status, 400);
}
