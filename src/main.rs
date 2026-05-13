use bilive_chat::bilibili::web_live::{HttpClient, LiveConnection};
use bilive_chat::config::ConfigStore;
use bilive_chat::overlay;
use std::path::PathBuf;
use std::sync::Arc;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let store = Arc::new(ConfigStore::new(PathBuf::from("data")));
    store.load_config()?;
    store.load_login_state()?;

    let config = store.config.lock().unwrap().clone();
    tracing::info!(
        "loaded config: room_id={}, host={}, port={}",
        config.room_id,
        config.host,
        config.port
    );

    let shared = overlay::state::new();

    let http_client = HttpClient::new();
    let live = LiveConnection::new(http_client, shared.panel_tx.clone());

    let router = overlay::server::build_router(shared, store, live);

    let addr = format!("{}:{}", config.host, config.port);
    tracing::info!("starting server on {addr}");

    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, router).await?;

    Ok(())
}
