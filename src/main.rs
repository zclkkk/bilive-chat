mod app;
mod bilibili;
mod chat;
mod config;
mod overlay;

use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let router = overlay::server::build_router();

    let addr = "127.0.0.1:7792";
    tracing::info!("starting server on {addr}");

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, router).await.unwrap();
}
