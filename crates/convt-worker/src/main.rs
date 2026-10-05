//! Pulls jobs from the queue, converts them with the same engines as the
//! desktop app, and uploads results to object storage. The queue (Postgres
//! `SKIP LOCKED`) and storage client aren't wired up yet.

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();
    let registry = convt_engines::default_registry();
    for engine in registry.engines() {
        tracing::info!(engine = engine.id(), "engine ready");
    }
    for (engine, reason) in registry.unavailable() {
        tracing::warn!(engine, reason, "engine unavailable");
    }
    tracing::info!("worker idle; job queue not implemented yet");
    tokio::signal::ctrl_c().await?;
    Ok(())
}
