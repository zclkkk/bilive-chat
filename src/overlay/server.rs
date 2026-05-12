use axum::Router;

const PANEL_HTML: &str = include_str!("../../web/panel.html");
const OVERLAY_HTML: &str = include_str!("../../web/overlay.html");
const PANEL_CSS: &str = include_str!("../../web/panel.css");
const PANEL_JS: &str = include_str!("../../web/panel.js");
const OVERLAY_CSS: &str = include_str!("../../web/overlay.css");
const OVERLAY_JS: &str = include_str!("../../web/overlay.js");

pub fn build_router() -> Router {
    Router::new()
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
        )
}
