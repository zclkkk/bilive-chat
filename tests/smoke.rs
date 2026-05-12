use bilive_chat::overlay::{server, state};
use futures_util::StreamExt;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio_tungstenite::connect_async;

async fn spawn_server() -> (String, u16) {
    let s = state::new();
    state::spawn_synthetic_messages(s.clone());

    let router = server::build_router(s);
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();

    tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });

    (format!("http://127.0.0.1:{port}"), port)
}

async fn http_get(port: u16, path: &str) -> (u16, String) {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).await.unwrap();
    let request = format!("GET {path} HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n");
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

#[tokio::test]
async fn panel_page_returns_200() {
    let (_base, port) = spawn_server().await;
    let (status, body) = http_get(port, "/").await;
    assert_eq!(status, 200);
    assert!(body.contains("bilive-chat"));
}

#[tokio::test]
async fn overlay_page_returns_200() {
    let (_base, port) = spawn_server().await;
    let (status, body) = http_get(port, "/overlay").await;
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
