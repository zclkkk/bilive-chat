use axum::extract::Query;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::{delete, get, post};
use axum::{Extension, Json, Router};
use std::collections::HashMap;
use std::sync::Arc;

use super::state::SharedState;
use crate::config::{Config, ConfigStore, LoginState};

const PANEL_HTML: &str = include_str!("../../web/panel.html");
const OVERLAY_HTML: &str = include_str!("../../web/overlay.html");
const PANEL_CSS: &str = include_str!("../../web/panel.css");
const PANEL_JS: &str = include_str!("../../web/panel.js");
const OVERLAY_CSS: &str = include_str!("../../web/overlay.css");
const OVERLAY_JS: &str = include_str!("../../web/overlay.js");

pub fn build_router(shared: SharedState, store: Arc<ConfigStore>) -> Router {
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
        .layer(Extension(store))
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

async fn get_overlay_url(
    Extension(store): Extension<Arc<ConfigStore>>,
    Query(params): Query<HashMap<String, String>>,
) -> impl IntoResponse {
    let config = store.config.lock().unwrap().clone();
    let overlay = &config.overlay;

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
        "http://{}:{}/overlay?max_items={}&lifetime={}&show_avatar={}&font_size={}",
        config.host, config.port, max_items, lifetime, show_avatar, font_size
    );

    Json(serde_json::json!({ "url": url }))
}

fn now_secs() -> String {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs().to_string())
        .unwrap_or_else(|_| "0".to_string())
}
