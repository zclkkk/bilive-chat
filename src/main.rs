use bilive_chat::overlay;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let state = overlay::state::new();
    overlay::state::spawn_synthetic_messages(state.clone());

    let router = overlay::server::build_router(state);

    let addr = "127.0.0.1:7792";
    tracing::info!("starting server on {addr}");

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, router).await?;

    Ok(())
}
