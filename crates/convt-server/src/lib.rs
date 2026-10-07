//! convt cloud API. The binary in `main.rs` serves [`app`]; the database modules
//! are shared with P9's job routes and with convt-worker later.

pub mod api_keys;
pub mod cleanup;
pub mod db;
pub mod ids;
pub mod jobs;
pub mod meter;
pub mod migrations;
pub mod routes;
pub mod storage;
pub mod tokens;
pub mod usage;

#[cfg(test)]
mod cloud_tests;
#[cfg(test)]
mod db_tests;
#[cfg(test)]
mod testing;

use axum::routing::get;
use axum::{Json, Router};
use serde::Serialize;
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;

/// Selects rustls's crypto provider. Call once at startup, before any TLS
/// connection (Postgres with `sslmode`, object storage, Polar).
pub fn install_crypto() {
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
}

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
        .route(
            "/openapi.json",
            get(|| async {
                Json(
                    serde_json::from_str::<serde_json::Value>(include_str!("../openapi.json"))
                        .unwrap(),
                )
            }),
        )
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http())
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
