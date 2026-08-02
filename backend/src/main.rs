mod config;
mod db;
mod fetchers;
mod ics;
mod models;
mod notify;
mod web;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "ov_watcher=info,tower_http=info".into()),
        )
        .init();
    let config = config::Config::from_env()?;
    tracing::info!(port = config.port, "config loaded");
    Ok(())
}
