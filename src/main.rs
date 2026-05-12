use bilive_chat::config::{Config, LoginState};
use bilive_chat::overlay;
use std::sync::{Arc, Mutex};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let config = Config::load();
    let login_state = LoginState::load();

    tracing::info!(
        "loaded config: room_id={}, host={}, port={}",
        config.room_id,
        config.host,
        config.port
    );

    let shared = overlay::state::new();
    overlay::state::spawn_synthetic_messages(shared.clone());

    let router = overlay::server::build_router(
        shared,
        Arc::new(Mutex::new(config)),
        Arc::new(Mutex::new(login_state)),
    );

    let addr = "127.0.0.1:7792";
    tracing::info!("starting server on {addr}");

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, router).await?;

    Ok(())
}
