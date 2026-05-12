use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::{delete, get, post};
use axum::{Extension, Json, Router};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use super::state::SharedState;
use crate::config::{Config, LoginState};

const PANEL_HTML: &str = include_str!("../../web/panel.html");
const OVERLAY_HTML: &str = include_str!("../../web/overlay.html");
const PANEL_CSS: &str = include_str!("../../web/panel.css");
const PANEL_JS: &str = include_str!("../../web/panel.js");
const OVERLAY_CSS: &str = include_str!("../../web/overlay.css");
const OVERLAY_JS: &str = include_str!("../../web/overlay.js");

pub struct AppState {
    pub config: Mutex<Config>,
    pub login_state: Mutex<LoginState>,
    pub data_dir: PathBuf,
}

pub fn build_router(shared: SharedState, app: Arc<AppState>) -> Router {
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
        .layer(Extension(app))
        .with_state(shared)
}

async fn get_config(Extension(app): Extension<Arc<AppState>>) -> impl IntoResponse {
    let config = app.config.lock().unwrap().clone();
    Json(config)
}

async fn post_config(
    Extension(app): Extension<Arc<AppState>>,
    Json(new_config): Json<Config>,
) -> impl IntoResponse {
    if let Err(e) = new_config.validate() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": e})),
        )
            .into_response();
    }
    if let Err(e) = new_config.save(Path::new(&app.data_dir)) {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e})),
        )
            .into_response();
    }
    *app.config.lock().unwrap() = new_config;
    (StatusCode::OK, Json(serde_json::json!({"ok": true}))).into_response()
}

async fn post_login_state(
    Extension(app): Extension<Arc<AppState>>,
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

    if let Err(e) = state.save(Path::new(&app.data_dir)) {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e})),
        )
            .into_response();
    }
    *app.login_state.lock().unwrap() = state;
    tracing::info!("login state saved");
    (StatusCode::OK, Json(serde_json::json!({"ok": true}))).into_response()
}

async fn delete_login_state(Extension(app): Extension<Arc<AppState>>) -> impl IntoResponse {
    if let Err(e) = LoginState::delete(Path::new(&app.data_dir)) {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e})),
        )
            .into_response();
    }
    *app.login_state.lock().unwrap() = LoginState::default();
    tracing::info!("login state deleted");
    (StatusCode::OK, Json(serde_json::json!({"ok": true}))).into_response()
}

fn now_secs() -> String {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs().to_string())
        .unwrap_or_else(|_| "0".to_string())
}
