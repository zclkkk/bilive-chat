use bilive_chat::overlay::{server, state};
use futures_util::StreamExt;
use tokio::net::TcpListener;
use tokio_tungstenite::connect_async;

async fn spawn_server() -> String {
    let s = state::new();
    state::spawn_synthetic_messages(s.clone());

    let router = server::build_router(s);
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });

    format!("http://127.0.0.1:{}", addr.port())
}

#[tokio::test]
async fn panel_page_returns_200() {
    let base = spawn_server().await;
    let resp = reqwest::get(format!("{base}/")).await.unwrap();
    assert_eq!(resp.status(), 200);
    let body = resp.text().await.unwrap();
    assert!(body.contains("bilive-chat"));
}

#[tokio::test]
async fn overlay_page_returns_200() {
    let base = spawn_server().await;
    let resp = reqwest::get(format!("{base}/overlay")).await.unwrap();
    assert_eq!(resp.status(), 200);
    let body = resp.text().await.unwrap();
    assert!(body.contains("chat-container"));
}

#[tokio::test]
async fn ws_panel_accepts_client() {
    let base = spawn_server().await;
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
    let base = spawn_server().await;
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
