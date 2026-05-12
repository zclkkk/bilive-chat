mod app;
mod bilibili;
mod chat;
mod config;
mod overlay;

use axum::Router;
use tracing_subscriber::EnvFilter;

const PANEL_HTML: &str = include_str!("../web/panel.html");
const OVERLAY_HTML: &str = include_str!("../web/overlay.html");
const PANEL_CSS: &str = include_str!("../web/panel.css");
const PANEL_JS: &str = include_str!("../web/panel.js");
const OVERLAY_CSS: &str = include_str!("../web/overlay.css");
const OVERLAY_JS: &str = include_str!("../web/overlay.js");

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let router = Router::new()
        .route(
            "/",
            axum::routing::get(|| async { axum::response::Html(PANEL_HTML) }),
        )
        .route(
            "/overlay",
            axum::routing::get(|| async { axum::response::Html(OVERLAY_HTML) }),
        )
        .route(
            "/panel.css",
            axum::routing::get(|| async { ([("content-type", "text/css")], PANEL_CSS) }),
        )
        .route(
            "/panel.js",
            axum::routing::get(|| async {
                ([("content-type", "application/javascript")], PANEL_JS)
            }),
        )
        .route(
            "/overlay.css",
            axum::routing::get(|| async { ([("content-type", "text/css")], OVERLAY_CSS) }),
        )
        .route(
            "/overlay.js",
            axum::routing::get(|| async {
                ([("content-type", "application/javascript")], OVERLAY_JS)
            }),
        );

    let addr = "127.0.0.1:7792";
    tracing::info!("starting server on {addr}");

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, router).await.unwrap();
}
