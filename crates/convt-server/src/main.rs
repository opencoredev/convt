//! Cloud API. Conversion processes live on a separate sandbox-capable host.
use std::sync::Arc;
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();
    let mut app = convt_server::app();
    if let Ok(url) = std::env::var("DATABASE_URL") {
        let pool = convt_server::db::connect(&url).await?;
        convt_server::migrations::check(&pool).await?;
        convt_server::cleanup::spawn(pool.clone());
        let secret = std::env::var("CONVT_WEB_TOKEN_SECRET")?;
        anyhow::ensure!(
            secret.len() >= 32,
            "CONVT_WEB_TOKEN_SECRET must be at least 32 characters"
        );
        anyhow::ensure!(
            std::env::var("CONVT_SANDBOX_VERIFIED").as_deref() == Ok("1"),
            "Run the sandbox gate on the worker host before enabling jobs; then set CONVT_SANDBOX_VERIFIED=1"
        );
        let state = convt_server::routes::CloudState {
            pool,
            storage: Arc::new(convt_server::storage::S3Storage::from_env()?),
            web_secret: secret,
        };
        app = app.merge(convt_server::routes::router(state.clone()));
        tokio::spawn(async move {
            let mut tick = tokio::time::interval(std::time::Duration::from_secs(60));
            loop {
                tick.tick().await;
                if let Err(e) = convt_server::routes::expire(&state).await {
                    tracing::warn!(error=%e,"expiry cleanup failed");
                }
            }
        });
    } else {
        tracing::warn!("DATABASE_URL absent; jobs disabled");
    }
    let port = std::env::var("PORT").unwrap_or_else(|_| "8080".into());
    let host = std::env::var("CONVT_BIND_HOST").unwrap_or_else(|_| "127.0.0.1".into());
    let listener = tokio::net::TcpListener::bind(format!("{host}:{port}")).await?;
    tracing::info!("listening on {}", listener.local_addr()?);
    axum::serve(listener, app)
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await?;
    Ok(())
}
