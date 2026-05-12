use bilive_chat::config::{Config, LoginState};
use bilive_chat::overlay;
use bilive_chat::overlay::server::AppState;
use std::path::PathBuf;
use std::sync::Arc;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let data_dir = PathBuf::from("data");
    let config = Config::load(&data_dir)?;
    let login_state = LoginState::load(&data_dir)?;

    tracing::info!(
        "loaded config: room_id={}, host={}, port={}",
        config.room_id,
        config.host,
        config.port
    );

    let shared = overlay::state::new();
    overlay::state::spawn_synthetic_messages(shared.clone());

    let app = Arc::new(AppState {
        config: std::sync::Mutex::new(config.clone()),
        login_state: std::sync::Mutex::new(login_state),
        data_dir,
    });

    let router = overlay::server::build_router(shared, app);

    let addr = format!("{}:{}", config.host, config.port);
    tracing::info!("starting server on {addr}");

    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, router).await?;

    Ok(())
}
