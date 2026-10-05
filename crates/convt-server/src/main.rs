use axum::routing::get;
use axum::{Json, Router};
use serde::Serialize;
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;

#[derive(Serialize)]
struct FormatInfo {
    id: &'static str,
    name: &'static str,
    category: String,
    extensions: &'static [&'static str],
    mime: &'static str,
}

async fn formats() -> Json<Vec<FormatInfo>> {
    Json(
        convt_core::FORMATS
            .iter()
            .map(|f| FormatInfo {
                id: f.id,
                name: f.name,
                category: format!("{:?}", f.category).to_lowercase(),
                extensions: f.extensions,
                mime: f.mime,
            })
            .collect(),
    )
}

pub fn app() -> Router {
    Router::new()
        .route("/health", get(|| async { "ok" }))
        .route("/v1/formats", get(formats))
        // TODO: POST /v1/jobs (upload, enqueue for convt-worker), GET /v1/jobs/{id}
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http())
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();
    // Railway injects PORT.
    let port = std::env::var("PORT").unwrap_or_else(|_| "8080".into());
    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{port}")).await?;
    tracing::info!("listening on {}", listener.local_addr()?);
    axum::serve(listener, app()).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::app;
    use axum::body::{Body, to_bytes};
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt as _;

    async fn get(path: &str) -> (StatusCode, Vec<u8>) {
        let res = app()
            .oneshot(Request::get(path).body(Body::empty()).unwrap())
            .await
            .unwrap();
        let status = res.status();
        (
            status,
            to_bytes(res.into_body(), usize::MAX)
                .await
                .unwrap()
                .to_vec(),
        )
    }

    #[tokio::test]
    async fn health() {
        assert_eq!(get("/health").await, (StatusCode::OK, b"ok".to_vec()));
    }

    #[tokio::test]
    async fn formats_lists_every_format() {
        let (status, body) = get("/v1/formats").await;
        assert_eq!(status, StatusCode::OK);
        let list: Vec<serde_json::Value> = serde_json::from_slice(&body).unwrap();
        assert_eq!(list.len(), convt_core::FORMATS.len());
        let png = list.iter().find(|f| f["id"] == "png").unwrap();
        assert_eq!(png["category"], "image");
        assert_eq!(png["mime"], "image/png");
        assert!(
            png["extensions"]
                .as_array()
                .unwrap()
                .contains(&"png".into())
        );
    }

    #[tokio::test]
    async fn unknown_route_is_404() {
        assert_eq!(get("/nope").await.0, StatusCode::NOT_FOUND);
    }
}
