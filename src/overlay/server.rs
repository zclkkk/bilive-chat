use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::{delete, get, post};
use axum::{Extension, Json, Router};
use std::sync::{Arc, Mutex};

use super::state::SharedState;
use crate::config::{Config, LoginState};

const PANEL_HTML: &str = include_str!("../../web/panel.html");
const OVERLAY_HTML: &str = include_str!("../../web/overlay.html");
const PANEL_CSS: &str = include_str!("../../web/panel.css");
const PANEL_JS: &str = include_str!("../../web/panel.js");
const OVERLAY_CSS: &str = include_str!("../../web/overlay.css");
const OVERLAY_JS: &str = include_str!("../../web/overlay.js");

pub fn build_router(
    shared: SharedState,
    config: Arc<Mutex<Config>>,
    login_state: Arc<Mutex<LoginState>>,
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
        .layer(Extension(config))
        .layer(Extension(login_state))
        .with_state(shared)
}

async fn get_config(Extension(config): Extension<Arc<Mutex<Config>>>) -> impl IntoResponse {
    let config = config.lock().unwrap().clone();
    Json(config)
}

async fn post_config(
    Extension(config): Extension<Arc<Mutex<Config>>>,
    Json(new_config): Json<Config>,
) -> impl IntoResponse {
    {
        let mut config = config.lock().unwrap();
        *config = new_config.clone();
    }
    match new_config.save() {
        Ok(()) => (StatusCode::OK, Json(serde_json::json!({"ok": true}))).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e})),
        )
            .into_response(),
    }
}

async fn post_login_state(
    Extension(login): Extension<Arc<Mutex<LoginState>>>,
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
        updated: Some(chrono_like_now()),
    };

    {
        let mut login = login.lock().unwrap();
        *login = state.clone();
    }

    tracing::info!("login state saved");
    match state.save() {
        Ok(()) => (StatusCode::OK, Json(serde_json::json!({"ok": true}))).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e})),
        )
            .into_response(),
    }
}

async fn delete_login_state(
    Extension(login): Extension<Arc<Mutex<LoginState>>>,
) -> impl IntoResponse {
    {
        let mut login = login.lock().unwrap();
        *login = LoginState::default();
    }

    tracing::info!("login state deleted");
    match LoginState::delete() {
        Ok(()) => (StatusCode::OK, Json(serde_json::json!({"ok": true}))).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e})),
        )
            .into_response(),
    }
}

fn chrono_like_now() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap();
    format!("{}", now.as_secs())
}
