use axum::extract::Query;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::{delete, get, post};
use axum::{Extension, Json, Router};
use std::collections::HashMap;
use std::sync::Arc;

use super::state::SharedState;
use crate::bilibili::web_live::{LiveConnection, StartError};
use crate::config::{Config, ConfigStore, LoginState};

const PANEL_HTML: &str = include_str!("../../web/panel.html");
const OVERLAY_HTML: &str = include_str!("../../web/overlay.html");
const PANEL_CSS: &str = include_str!("../../web/panel.css");
const PANEL_JS: &str = include_str!("../../web/panel.js");
const OVERLAY_CSS: &str = include_str!("../../web/overlay.css");
const OVERLAY_JS: &str = include_str!("../../web/overlay.js");

pub fn build_router(
    shared: SharedState,
    store: Arc<ConfigStore>,
    live: Arc<LiveConnection>,
) -> Router {
    Router::new()
        .route("/", get(|| async { axum::response::Html(PANEL_HTML) }))
        .route(
            "/overlay",
            get(|| async { axum::response::Html(OVERLAY_HTML) }),
        )
        .route(
            "/panel.css",
            get(|| async { ([("content-type", "text/css")], PANEL_CSS) }),
        )
        .route(
            "/panel.js",
            get(|| async { ([("content-type", "application/javascript")], PANEL_JS) }),
        )
        .route(
            "/overlay.css",
            get(|| async { ([("content-type", "text/css")], OVERLAY_CSS) }),
        )
        .route(
            "/overlay.js",
            get(|| async { ([("content-type", "application/javascript")], OVERLAY_JS) }),
        )
        .route("/ws/panel", get(super::ws::panel))
        .route("/ws/overlay", get(super::ws::overlay))
        .route("/api/config", get(get_config))
        .route("/api/config", post(post_config))
        .route("/api/bilibili/login-state", post(post_login_state))
        .route("/api/bilibili/login-state", delete(delete_login_state))
        .route("/api/overlay-url", get(get_overlay_url))
        .route("/api/bilibili/start", post(post_start))
        .route("/api/bilibili/stop", post(post_stop))
        .route("/api/bilibili/status", get(get_status))
        .layer(Extension(store))
        .layer(Extension(live))
        .with_state(shared)
}

async fn get_config(Extension(store): Extension<Arc<ConfigStore>>) -> impl IntoResponse {
    let config = store.config.lock().unwrap().clone();
    Json(config)
}

async fn post_config(
    Extension(store): Extension<Arc<ConfigStore>>,
    Json(new_config): Json<Config>,
) -> impl IntoResponse {
    if let Err(e) = new_config.validate() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": e})),
        )
            .into_response();
    }
    if let Err(e) = store.save_config(&new_config) {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e})),
        )
            .into_response();
    }
    (StatusCode::OK, Json(serde_json::json!({"ok": true}))).into_response()
}

async fn post_login_state(
    Extension(store): Extension<Arc<ConfigStore>>,
    Json(body): Json<serde_json::Value>,
) -> impl IntoResponse {
    let cookie = match body.get("cookie").and_then(|v| v.as_str()) {
        Some(c) => c.to_string(),
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": "missing cookie"})),
            )
                .into_response()
        }
    };

    let state = LoginState {
        cookie,
        updated: Some(now_secs()),
    };

    if let Err(e) = store.save_login_state(&state) {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e})),
        )
            .into_response();
    }
    tracing::info!("login state saved");
    (StatusCode::OK, Json(serde_json::json!({"ok": true}))).into_response()
}

async fn delete_login_state(Extension(store): Extension<Arc<ConfigStore>>) -> impl IntoResponse {
    if let Err(e) = store.delete_login_state() {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e})),
        )
            .into_response();
    }
    tracing::info!("login state deleted");
    (StatusCode::OK, Json(serde_json::json!({"ok": true}))).into_response()
}

async fn post_start(
    Extension(store): Extension<Arc<ConfigStore>>,
    Extension(live): Extension<Arc<LiveConnection>>,
) -> impl IntoResponse {
    let config = store.config.lock().unwrap().clone();
    let room_id = config.room_id;
    if room_id == 0 {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "room_id is 0"})),
        )
            .into_response();
    }

    let cookie = {
        let state = store.login_state.lock().unwrap();
        if state.cookie.is_empty() {
            None
        } else {
            Some(state.cookie.clone())
        }
    };

    match live.start(room_id, cookie).await {
        Ok(()) => {
            tracing::info!("web_live started for room {room_id}");
            (StatusCode::OK, Json(serde_json::json!({"ok": true}))).into_response()
        }
        Err(StartError::AlreadyRunning) => (
            StatusCode::CONFLICT,
            Json(serde_json::json!({"error": "already running"})),
        )
            .into_response(),
        Err(StartError::CookieNotLoggedIn) => (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({"error": "cookie present but not logged in"})),
        )
            .into_response(),
        Err(StartError::Auth(e)) => (
            StatusCode::BAD_GATEWAY,
            Json(serde_json::json!({"error": e.to_string()})),
        )
            .into_response(),
    }
}

async fn post_stop(Extension(live): Extension<Arc<LiveConnection>>) -> impl IntoResponse {
    if live.stop().await {
        tracing::info!("web_live stopped");
        (StatusCode::OK, Json(serde_json::json!({"ok": true}))).into_response()
    } else {
        (
            StatusCode::CONFLICT,
            Json(serde_json::json!({"error": "not running"})),
        )
            .into_response()
    }
}

async fn get_status(Extension(live): Extension<Arc<LiveConnection>>) -> impl IntoResponse {
    let status = live.status().await;
    Json(serde_json::to_value(status).unwrap_or_default())
}

async fn get_overlay_url(
    Extension(store): Extension<Arc<ConfigStore>>,
    headers: axum::http::HeaderMap,
    Query(params): Query<HashMap<String, String>>,
) -> impl IntoResponse {
    let config = store.config.lock().unwrap().clone();
    let overlay = &config.overlay;

    let host = headers
        .get("host")
        .and_then(|v| v.to_str().ok())
        .map(|h| h.to_string())
        .unwrap_or_else(|| {
            let bind_host = if config.host == "0.0.0.0" {
                "127.0.0.1"
            } else {
                &config.host
            };
            format!("{}:{}", bind_host, config.port)
        });

    let max_items = params
        .get("max_items")
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(overlay.max_items);
    let lifetime = params
        .get("lifetime")
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(overlay.message_lifetime_secs);
    let show_avatar = params
        .get("show_avatar")
        .map(|v| v == "true" || v == "1")
        .unwrap_or(overlay.show_avatar);
    let font_size = params
        .get("font_size")
        .and_then(|v| v.parse::<u32>().ok())
        .unwrap_or(14);

    let url = format!(
        "http://{host}/overlay?max_items={max_items}&lifetime={lifetime}&show_avatar={show_avatar}&font_size={font_size}"
    );

    Json(serde_json::json!({ "url": url }))
}

fn now_secs() -> String {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs().to_string())
        .unwrap_or_else(|_| "0".to_string())
}
