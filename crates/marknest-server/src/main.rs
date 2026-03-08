use std::env;
use std::sync::Arc;

use marknest_server::{ChromiumFallbackExporter, app};
use tracing_subscriber::{EnvFilter, layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    init_tracing();

    let bind_address =
        env::var("MARKNEST_SERVER_ADDR").unwrap_or_else(|_| "127.0.0.1:3476".to_string());
    let listener = tokio::net::TcpListener::bind(&bind_address).await?;

    tracing::info!(bind_address = %bind_address, "MarkNest fallback server listening");

    axum::serve(listener, app(Arc::new(ChromiumFallbackExporter))).await?;

    Ok(())
}

fn init_tracing() {
    let env_filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("marknest_server=info,tower_http=info"));

    tracing_subscriber::registry()
        .with(env_filter)
        .with(tracing_subscriber::fmt::layer().compact())
        .init();
}
