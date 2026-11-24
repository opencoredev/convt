//! convt cloud API. The binary in `main.rs` serves [`app`]; the database modules
//! are shared with P9's job routes and with convt-worker later.

pub mod api_keys;
pub mod cleanup;
pub mod convert;
pub mod db;
pub mod ids;
pub mod jobs;
pub mod meter;
pub mod migrations;
pub mod net_guard;
pub mod routes;
pub mod storage;
pub mod tokens;
pub mod usage;
pub mod webhooks;

#[cfg(test)]
mod cloud_tests;
#[cfg(test)]
mod db_tests;
#[cfg(test)]
mod testing;

use axum::extract::Request;
use axum::http::{HeaderValue, StatusCode};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
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

/// The public routes alone: health, formats and the OpenAPI document.
pub fn app() -> Router {
    service(None)
}

/// Every route, with request ids, tracing and JSON 404s around all of them.
/// `jobs` is the authenticated router from [`routes::router`] when a database is
/// configured.
pub fn service(jobs: Option<Router>) -> Router {
    let mut router = Router::new()
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
        );
    if let Some(jobs) = jobs {
        router = router.merge(jobs);
    }
    router
        .fallback(|| async {
            routes::ApiError::not_found(
                "No such route. See https://convt.app/docs/api for every route.",
            )
        })
        .method_not_allowed_fallback(|| async {
            routes::ApiError::new(
                StatusCode::METHOD_NOT_ALLOWED,
                "method_not_allowed",
                "This route does not take that method. See https://convt.app/docs/api.",
            )
        })
        .layer(CorsLayer::permissive())
        .layer(
            TraceLayer::new_for_http().make_span_with(|request: &Request| {
                let id = request
                    .headers()
                    .get("x-request-id")
                    .and_then(|v| v.to_str().ok())
                    .unwrap_or_default();
                tracing::info_span!("request", method = %request.method(), path = %request.uri().path(), request_id = id)
            }),
        )
        .layer(middleware::from_fn(request_id))
}

/// Keeps a caller's `X-Request-Id` when it is short and plain, otherwise makes
/// one, and returns it on every response and in every JSON error.
async fn request_id(mut request: Request, next: Next) -> Response {
    let id = request
        .headers()
        .get("x-request-id")
        .and_then(|v| v.to_str().ok())
        .filter(|v| {
            !v.is_empty()
                && v.len() <= 128
                && v.bytes()
                    .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
        })
        .map(str::to_owned)
        .unwrap_or_else(|| ids::new_id("req"));
    let value = HeaderValue::from_str(&id).expect("checked or generated ASCII");
    request.headers_mut().insert("x-request-id", value.clone());
    let mut response = routes::REQUEST_ID
        .scope(id, next.run(request))
        .await
        .into_response();
    response.headers_mut().insert("x-request-id", value);
    response
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
    async fn unknown_route_is_a_json_404_with_a_request_id() {
        let res = app()
            .oneshot(Request::get("/nope").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::NOT_FOUND);
        let id = res.headers()["x-request-id"].to_str().unwrap().to_owned();
        assert!(id.starts_with("req_"), "{id}");
        let body: serde_json::Value =
            serde_json::from_slice(&to_bytes(res.into_body(), usize::MAX).await.unwrap()).unwrap();
        assert_eq!(body["error"]["code"], "not_found");
        assert_eq!(body["error"]["request_id"], id.as_str());
    }

    #[tokio::test]
    async fn a_plain_caller_request_id_is_kept_and_odd_ones_replaced() {
        for (sent, kept) in [("trace-123.abc_X", true), ("bad id\twith spaces", false)] {
            let res = app()
                .oneshot(
                    Request::get("/health")
                        .header("x-request-id", sent)
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            let id = res.headers()["x-request-id"].to_str().unwrap();
            assert_eq!(id == sent, kept, "{id}");
        }
    }

    #[tokio::test]
    async fn wrong_method_is_a_json_405() {
        let res = app()
            .oneshot(Request::post("/v1/formats").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::METHOD_NOT_ALLOWED);
    }
}
