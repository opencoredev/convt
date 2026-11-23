//! Cloud API. Conversion processes live on a separate sandbox-capable host.
use std::sync::Arc;
use std::time::Duration;

/// How long in-flight requests get after SIGTERM before the process exits. Keep it
/// below the platform's drain window (`drainingSeconds` in railway.json is 30).
const DRAIN: Duration = Duration::from_secs(24);

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();
    let mut jobs = None;
    let mut dispatcher = None;
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
        let webhook_secret = std::env::var("CONVT_WEBHOOK_SECRET").ok();
        if let Some(value) = &webhook_secret {
            anyhow::ensure!(
                value.len() >= 32,
                "CONVT_WEBHOOK_SECRET must be at least 32 characters"
            );
        } else {
            tracing::warn!("CONVT_WEBHOOK_SECRET absent; webhook_url is refused");
        }
        let storage: Arc<dyn convt_server::storage::Storage> =
            Arc::new(convt_server::storage::S3Storage::from_env()?);
        let state = convt_server::routes::CloudState {
            pool: pool.clone(),
            storage: storage.clone(),
            web_secret: secret,
            webhook_secret: webhook_secret.clone(),
            source_urls: std::env::var("CONVT_SOURCE_URLS").as_deref() == Ok("1"),
        };
        if let Some(master) = webhook_secret {
            dispatcher = Some(convt_server::webhooks::spawn(
                pool,
                storage,
                master,
                convt_server::net_guard::Policy::from_env(),
            ));
        }
        jobs = Some(
            convt_server::routes::router(state.clone())
                .merge(convt_server::routes::ready_router(state.clone())),
        );
        tokio::spawn(async move {
            let mut tick = tokio::time::interval(Duration::from_secs(60));
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
    let app = convt_server::service(jobs);
    let port = std::env::var("PORT").unwrap_or_else(|_| "8080".into());
    let host = std::env::var("CONVT_BIND_HOST").unwrap_or_else(|_| "127.0.0.1".into());
    let listener = tokio::net::TcpListener::bind(format!("{host}:{port}")).await?;
    tracing::info!("listening on {}", listener.local_addr()?);
    let serve = axum::serve(listener, app).with_graceful_shutdown(async {
        shutdown_signal().await;
        tracing::info!("draining: no new connections, finishing in-flight requests");
        convt_server::routes::start_draining();
    });
    tokio::select! {
        result = serve => result?,
        () = async {
            shutdown_signal().await;
            tokio::time::sleep(DRAIN).await;
        } => tracing::warn!("drain window over; exiting with requests still open"),
    }
    // The dispatcher stops claiming once draining starts; let a pass in flight
    // record its results, so an acknowledged delivery is not sent again.
    if let Some(dispatcher) = dispatcher
        && tokio::time::timeout(Duration::from_secs(4), dispatcher)
            .await
            .is_err()
    {
        tracing::warn!("webhook dispatcher still sending at exit; its rows retry after the lease");
    }
    Ok(())
}

/// SIGTERM (Railway, Docker, Kubernetes, systemd) or Ctrl+C.
async fn shutdown_signal() {
    #[cfg(unix)]
    {
        let mut term = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("install the SIGTERM handler");
        tokio::select! {
            _ = term.recv() => {},
            _ = tokio::signal::ctrl_c() => {},
        }
    }
    #[cfg(not(unix))]
    {
        let _ = tokio::signal::ctrl_c().await;
    }
}
