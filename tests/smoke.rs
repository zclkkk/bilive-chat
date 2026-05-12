use bilive_chat::config::{Config, ConfigStore, LoginState};
use bilive_chat::overlay::{server, state};
use futures_util::StreamExt;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio_tungstenite::connect_async;

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "bilive-chat-{name}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

async fn spawn_server() -> (String, u16, PathBuf) {
    let data_dir = temp_dir("server");
    let shared = state::new();
    state::spawn_synthetic_messages(shared.clone());

    let store = Arc::new(ConfigStore::new(data_dir.clone()));
    let router = server::build_router(shared, store);
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();

    tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });

    (format!("http://127.0.0.1:{port}"), port, data_dir)
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

// HTTP route tests

#[tokio::test]
async fn panel_page_returns_200() {
    let (_base, port, _dir) = spawn_server().await;
    let (status, body) = http_request(port, "GET", "/", "").await;
    assert_eq!(status, 200);
    assert!(body.contains("bilive-chat"));
}

#[tokio::test]
async fn overlay_page_returns_200() {
    let (_base, port, _dir) = spawn_server().await;
    let (status, body) = http_request(port, "GET", "/overlay", "").await;
    assert_eq!(status, 200);
    assert!(body.contains("chat-container"));
}

// WebSocket tests

#[tokio::test]
async fn ws_panel_accepts_client() {
    let (base, _port, _dir) = spawn_server().await;
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
async fn ws_overlay_receives_display_events() {
    let (base, _port, _dir) = spawn_server().await;
    let ws_url = base.replace("http://", "ws://") + "/ws/overlay";
    let (mut ws, _) = connect_async(&ws_url).await.unwrap();

    let mut seen_types = std::collections::HashSet::new();
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(20);

    while seen_types.len() < 3 && tokio::time::Instant::now() < deadline {
        let msg = tokio::time::timeout(deadline - tokio::time::Instant::now(), ws.next())
            .await
            .expect("timeout waiting for overlay events")
            .unwrap()
            .unwrap();

        let text = match msg {
            tokio_tungstenite::tungstenite::Message::Text(t) => t,
            other => panic!("expected text, got {other:?}"),
        };

        let parsed: serde_json::Value = serde_json::from_str(&text).unwrap();
        let event_type = parsed["type"].as_str().unwrap().to_string();
        seen_types.insert(event_type.clone());

        match event_type.as_str() {
            "normal" => {
                assert!(parsed["sender"].as_str().is_some());
                assert!(parsed["text"].as_str().is_some());
                assert!(parsed["avatar_color"].as_str().is_some());
            }
            "gift" => {
                assert!(parsed["sender"].as_str().is_some());
                assert!(parsed["gift_name"].as_str().is_some());
                assert!(parsed["count"].as_u64().is_some());
            }
            "super_chat" => {
                assert!(parsed["sender"].as_str().is_some());
                assert!(parsed["amount"].as_u64().is_some());
                assert!(parsed["currency"].as_str().is_some());
            }
            "guard" => {
                assert!(parsed["sender"].as_str().is_some());
                assert!(parsed["guard_name"].as_str().is_some());
                assert!(parsed["count"].as_u64().is_some());
            }
            "system" => {
                assert!(parsed["text"].as_str().is_some());
            }
            other => panic!("unexpected event type: {other}"),
        }
    }

    assert!(
        seen_types.len() >= 3,
        "expected at least 3 distinct event types, got: {seen_types:?}"
    );
}

// Config API tests

#[tokio::test]
async fn api_config_get_returns_defaults() {
    let (_base, port, _dir) = spawn_server().await;
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
    let (_base, port, data_dir) = spawn_server().await;
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

    let saved: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(data_dir.join("config.json")).unwrap())
            .unwrap();
    assert_eq!(saved["host"], "0.0.0.0");
}

#[tokio::test]
async fn api_config_post_rejects_empty_host() {
    let (_base, port, _dir) = spawn_server().await;
    let body =
        serde_json::json!({"host": "", "port": 8080, "room_id": 0, "overlay": {}, "filter": {}})
            .to_string();
    let (status, _) = http_request(port, "POST", "/api/config", &body).await;
    assert_eq!(status, 400);
}

#[tokio::test]
async fn api_config_post_rejects_zero_port() {
    let (_base, port, _dir) = spawn_server().await;
    let body = serde_json::json!({"host": "127.0.0.1", "port": 0, "room_id": 0, "overlay": {}, "filter": {}}).to_string();
    let (status, _) = http_request(port, "POST", "/api/config", &body).await;
    assert_eq!(status, 400);
}

#[tokio::test]
async fn api_config_post_rejects_zero_max_items() {
    let (_base, port, _dir) = spawn_server().await;
    let body = serde_json::json!({"host": "127.0.0.1", "port": 8080, "room_id": 0, "overlay": {"max_items": 0}, "filter": {}}).to_string();
    let (status, _) = http_request(port, "POST", "/api/config", &body).await;
    assert_eq!(status, 400);
}

// Login state API tests

#[tokio::test]
async fn api_login_state_save_and_delete() {
    let (_base, port, data_dir) = spawn_server().await;

    let body = serde_json::json!({"cookie": "test_cookie_value"}).to_string();
    let (status, _) = http_request(port, "POST", "/api/bilibili/login-state", &body).await;
    assert_eq!(status, 200);

    let saved: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(data_dir.join("login-state.json")).unwrap())
            .unwrap();
    assert_eq!(saved["cookie"], "test_cookie_value");
    assert!(saved["updated"].as_str().is_some());

    let (status, _) = http_request(port, "DELETE", "/api/bilibili/login-state", "").await;
    assert_eq!(status, 200);
    assert!(!data_dir.join("login-state.json").exists());
}

#[tokio::test]
async fn api_login_state_post_missing_cookie() {
    let (_base, port, _dir) = spawn_server().await;
    let (status, _) = http_request(port, "POST", "/api/bilibili/login-state", "{}").await;
    assert_eq!(status, 400);
}

// ConfigStore sync tests

#[test]
fn config_load_missing_file_returns_defaults() {
    let dir = temp_dir("cfg-missing");
    let store = ConfigStore::new(dir.clone());
    store.load_config().unwrap();
    let cfg = store.config.lock().unwrap().clone();
    assert_eq!(cfg.host, "127.0.0.1");
    assert_eq!(cfg.port, 7792);
}

#[test]
fn config_load_invalid_json_returns_error() {
    let dir = temp_dir("cfg-badjson");
    std::fs::write(dir.join("config.json"), "not json").unwrap();
    let store = ConfigStore::new(dir);
    let result = store.load_config();
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("invalid config"));
}

#[test]
fn config_save_and_load_roundtrip() {
    let dir = temp_dir("cfg-roundtrip");
    let store = ConfigStore::new(dir.clone());
    let cfg = Config {
        room_id: 42,
        ..Config::default()
    };
    store.save_config(&cfg).unwrap();
    store.load_config().unwrap();
    let loaded = store.config.lock().unwrap().clone();
    assert_eq!(loaded.room_id, 42);
}

#[test]
fn config_atomic_write_no_tmp_leftover() {
    let dir = temp_dir("cfg-atomic");
    let store = ConfigStore::new(dir.clone());
    store.save_config(&Config::default()).unwrap();
    assert!(!dir.join("config.json.tmp").exists());
    assert!(dir.join("config.json").exists());
}

#[test]
fn login_state_load_missing_file_returns_default() {
    let dir = temp_dir("ls-missing");
    let store = ConfigStore::new(dir);
    store.load_login_state().unwrap();
    let ls = store.login_state.lock().unwrap().clone();
    assert!(ls.cookie.is_empty());
}

#[test]
fn login_state_load_invalid_json_returns_error() {
    let dir = temp_dir("ls-badjson");
    std::fs::write(dir.join("login-state.json"), "{bad}").unwrap();
    let store = ConfigStore::new(dir);
    let result = store.load_login_state();
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("invalid login state"));
}

#[test]
fn login_state_save_load_delete() {
    let dir = temp_dir("ls-cycle");
    let store = ConfigStore::new(dir.clone());
    let ls = LoginState {
        cookie: "abc".into(),
        updated: Some("123".into()),
    };
    store.save_login_state(&ls).unwrap();
    store.load_login_state().unwrap();
    let loaded = store.login_state.lock().unwrap().clone();
    assert_eq!(loaded.cookie, "abc");
    store.delete_login_state().unwrap();
    assert!(!dir.join("login-state.json").exists());
}

#[test]
fn config_validate_rejects_empty_host() {
    let cfg = Config {
        host: String::new(),
        ..Config::default()
    };
    assert!(cfg.validate().is_err());
}

#[test]
fn config_validate_rejects_zero_port() {
    let cfg = Config {
        port: 0,
        ..Config::default()
    };
    assert!(cfg.validate().is_err());
}

// Overlay URL API tests

#[tokio::test]
async fn api_overlay_url_returns_default() {
    let (_base, port, _dir) = spawn_server().await;
    let (status, resp) = http_request(port, "GET", "/api/overlay-url", "").await;
    assert_eq!(status, 200);
    let body = response_body(&resp);
    let data: serde_json::Value = serde_json::from_str(body).unwrap();
    let url = data["url"].as_str().unwrap();
    assert!(url.contains("/overlay?"));
    assert!(url.contains("max_items=50"));
    assert!(url.contains("lifetime=300"));
    assert!(url.contains("show_avatar=true"));
    assert!(url.contains("font_size=14"));
}

#[tokio::test]
async fn api_overlay_url_with_query_params() {
    let (_base, port, _dir) = spawn_server().await;
    let (status, resp) = http_request(
        port,
        "GET",
        "/api/overlay-url?max_items=10&font_size=18",
        "",
    )
    .await;
    assert_eq!(status, 200);
    let body = response_body(&resp);
    let data: serde_json::Value = serde_json::from_str(body).unwrap();
    let url = data["url"].as_str().unwrap();
    assert!(url.contains("max_items=10"));
    assert!(url.contains("font_size=18"));
}
