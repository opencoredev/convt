//! `POST /v1/convert`: one request in, the converted file out.
//!
//! It runs the same pipeline as the jobs routes: the same reservation and spend
//! cap, the same sealed upload and size check, the same queue, workers and
//! metering. It only saves the client the upload, start, poll and download calls.
//!
//! The input is a multipart `file` field, a raw request body, or a JSON body with
//! `source_url`. When the job succeeds with one output the response is that file;
//! with several outputs, or `Accept: application/json`, it is JSON with signed
//! URLs. A job still running after `wait` seconds answers `202` with the job, which
//! keeps running and can be polled. Without an `Idempotency-Key`, closing the
//! connection before the answer cancels the job, so an abandoned request is not
//! billed; with one, the job keeps going so a retry with the same key picks it up.

use std::collections::HashMap;
use std::time::Duration;

use axum::{
    Extension, Json, Router,
    body::{Body, Bytes},
    extract::{DefaultBodyLimit, FromRequest, Multipart, Query, Request, State},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
    routing::post,
};
use sha2::{Digest, Sha256};

use crate::jobs::{self, CreateJob, Principal};
use crate::routes::{self, ApiError, CloudState, JobView};

/// Largest input `/v1/convert` takes in the request body. Larger files use the
/// jobs routes, which upload straight to storage, or `source_url`.
pub const MAX_INLINE_BYTES: usize = 25_000_000;
pub const DEFAULT_WAIT_SECONDS: u64 = 120;
pub const MAX_WAIT_SECONDS: u64 = 300;

pub fn router() -> Router<CloudState> {
    Router::new().route(
        "/v1/convert",
        post(convert).layer(DefaultBodyLimit::max(MAX_INLINE_BYTES + 64 * 1024)),
    )
}

enum Input {
    Bytes(Bytes),
    Url(String),
}

struct Parsed {
    to: Option<String>,
    from: Option<String>,
    options: Option<serde_json::Value>,
    filename: Option<String>,
    content_type: Option<String>,
    input: Option<Input>,
}

fn too_large() -> ApiError {
    ApiError::new(
        StatusCode::PAYLOAD_TOO_LARGE,
        "file_too_large",
        "POST /v1/convert takes files up to 25 MB. Use POST /v1/jobs or source_url for larger files.",
    )
}

/// A format id, or an extension that names one (`jpg` is `jpeg`).
fn format_id(value: &str) -> Option<&'static str> {
    let value = value.trim().trim_start_matches('.').to_ascii_lowercase();
    convt_core::FORMATS
        .iter()
        .find(|f| f.id == value)
        .or_else(|| {
            convt_core::FORMATS
                .iter()
                .find(|f| f.extensions.contains(&value.as_str()))
        })
        .map(|f| f.id)
}

fn format_from_mime(mime: &str) -> Option<&'static str> {
    let mime = mime.split(';').next()?.trim().to_ascii_lowercase();
    if mime.is_empty() || mime == "application/octet-stream" {
        return None;
    }
    convt_core::FORMATS
        .iter()
        .find(|f| f.mime == mime)
        .map(|f| f.id)
}

async fn read_body(request: Request, s: &CloudState) -> Result<Parsed, ApiError> {
    let content_type = request
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .map(str::to_owned);
    let mut parsed = Parsed {
        to: None,
        from: None,
        options: None,
        filename: None,
        content_type: content_type.clone(),
        input: None,
    };
    let kind = content_type
        .as_deref()
        .and_then(|c| c.split(';').next())
        .map(|c| c.trim().to_ascii_lowercase());
    match kind.as_deref() {
        Some("multipart/form-data") => {
            let mut form = Multipart::from_request(request, s)
                .await
                .map_err(|e| ApiError::bad("invalid_request", &e.body_text()))?;
            loop {
                let field = match form.next_field().await {
                    Ok(Some(field)) => field,
                    Ok(None) => break,
                    Err(e) if e.status() == StatusCode::PAYLOAD_TOO_LARGE => {
                        return Err(too_large());
                    }
                    Err(e) => return Err(ApiError::bad("invalid_request", &e.body_text())),
                };
                let name = field.name().unwrap_or_default().to_owned();
                match name.as_str() {
                    "file" => {
                        parsed.filename = field.file_name().map(str::to_owned);
                        parsed.content_type = field.content_type().map(str::to_owned);
                        let bytes = field.bytes().await.map_err(|e| {
                            if e.status() == StatusCode::PAYLOAD_TOO_LARGE {
                                too_large()
                            } else {
                                ApiError::bad("invalid_request", &e.body_text())
                            }
                        })?;
                        parsed.input = Some(Input::Bytes(bytes));
                    }
                    "to" | "from" | "options" | "source_url" => {
                        let text = field
                            .text()
                            .await
                            .map_err(|e| ApiError::bad("invalid_request", &e.body_text()))?;
                        match name.as_str() {
                            "to" => parsed.to = Some(text),
                            "from" => parsed.from = Some(text),
                            "source_url" => parsed.input = Some(Input::Url(text)),
                            _ => {
                                parsed.options = Some(serde_json::from_str(&text).map_err(|e| {
                                    ApiError::bad("invalid_options", &format!("options: {e}"))
                                })?)
                            }
                        }
                    }
                    other => {
                        return Err(ApiError::bad(
                            "invalid_request",
                            &format!(
                                "Unknown form field `{other}`. Send file, to, from or options."
                            ),
                        ));
                    }
                }
            }
        }
        Some("application/json") => {
            #[derive(serde::Deserialize)]
            #[serde(deny_unknown_fields)]
            struct JsonBody {
                source_url: String,
                to: Option<String>,
                from: Option<String>,
                options: Option<serde_json::Value>,
            }
            let bytes = Bytes::from_request(request, s)
                .await
                .map_err(|_| ApiError::bad("invalid_request", "Could not read the body."))?;
            let body: JsonBody = serde_path_to_error::deserialize(
                &mut serde_json::Deserializer::from_slice(&bytes),
            )
            .map_err(|e| {
                ApiError::bad(
                    "invalid_request",
                    &format!("{e}. A JSON body takes source_url, to, from and options; send files as multipart or a raw body."),
                )
            })?;
            parsed.to = body.to;
            parsed.from = body.from;
            parsed.options = body.options;
            parsed.input = Some(Input::Url(body.source_url));
            parsed.content_type = None;
        }
        _ => {
            let bytes = Bytes::from_request(request, s).await.map_err(|e| {
                if e.status() == StatusCode::PAYLOAD_TOO_LARGE {
                    too_large()
                } else {
                    ApiError::bad("invalid_request", "Could not read the body.")
                }
            })?;
            parsed.input = Some(Input::Bytes(bytes));
        }
    }
    Ok(parsed)
}

/// Cancels the job if the handler is dropped before it answers: the client hung
/// up. Disarmed once there is an answer or when the client sent an
/// `Idempotency-Key` (a retry may still want the job).
struct CancelOnDrop(Option<(sqlx::PgPool, Principal, String)>);
impl CancelOnDrop {
    fn disarm(&mut self) {
        self.0 = None;
    }
}
impl Drop for CancelOnDrop {
    fn drop(&mut self) {
        if let Some((pool, who, id)) = self.0.take()
            && let Ok(handle) = tokio::runtime::Handle::try_current()
        {
            handle.spawn(async move {
                if let Err(error) = jobs::cancel(&pool, &who, &id).await {
                    tracing::warn!(
                        job_id = id,
                        error = error.code(),
                        "cancel after disconnect failed"
                    );
                }
            });
        }
    }
}

fn job_error(status: StatusCode, code: &str, message: &str, job: jobs::Job) -> Response {
    let request_id = routes::REQUEST_ID.try_with(Clone::clone).ok();
    (
        status,
        Json(serde_json::json!({
            "error": {"code": code, "message": message, "request_id": request_id},
            "job": JobView::from(job),
        })),
    )
        .into_response()
}

/// `name.ext` from the uploaded file name's stem and the output's extension,
/// limited to characters every client handles in Content-Disposition.
fn download_name(input_name: Option<&str>, output_key: &str) -> String {
    let output = output_key.rsplit('/').next().unwrap_or("output");
    let extension = output.rsplit_once('.').map(|(_, e)| e).unwrap_or("bin");
    let stem = input_name
        .and_then(|n| n.rsplit(['/', '\\']).next())
        .map(|n| n.rsplit_once('.').map(|(s, _)| s).unwrap_or(n))
        .map(|n| {
            n.chars()
                .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | ' '))
                .take(100)
                .collect::<String>()
        })
        .filter(|n| !n.trim().is_empty())
        .unwrap_or_else(|| "output".into());
    format!("{}.{extension}", stem.trim())
}

async fn convert(
    State(s): State<CloudState>,
    Extension(who): Extension<Principal>,
    Query(query): Query<HashMap<String, String>>,
    headers: HeaderMap,
    request: Request,
) -> Result<Response, ApiError> {
    let wait = match query.get("wait") {
        None => DEFAULT_WAIT_SECONDS,
        Some(value) => value
            .parse::<u64>()
            .ok()
            .filter(|w| *w <= MAX_WAIT_SECONDS)
            .ok_or_else(|| {
                ApiError::bad(
                    "invalid_request",
                    "wait must be a whole number of seconds from 0 to 300.",
                )
            })?,
    };
    let mut body = read_body(request, &s).await?;
    body.to = body.to.or_else(|| query.get("to").cloned());
    body.from = body.from.or_else(|| query.get("from").cloned());
    body.filename = body.filename.or_else(|| query.get("filename").cloned());
    if body.options.is_none()
        && let Some(raw) = query.get("options")
    {
        body.options = Some(
            serde_json::from_str(raw)
                .map_err(|e| ApiError::bad("invalid_options", &format!("options: {e}")))?,
        );
    }
    let input = body.input.take().ok_or_else(|| {
        ApiError::bad(
            "invalid_request",
            "Send the file as a multipart `file` field or as the request body, or a JSON body with source_url.",
        )
    })?;
    let to = body.to.as_deref().ok_or_else(|| {
        ApiError::bad(
            "invalid_request",
            "Choose a target format with `to`, such as to=webp.",
        )
    })?;
    let target = format_id(to).ok_or_else(|| {
        ApiError::bad(
            "unsupported_format",
            &format!("`{to}` is not a format id. GET /v1/formats lists them."),
        )
    })?;
    let source_name = match &input {
        Input::Url(url) => Some(url.as_str()),
        Input::Bytes(_) => body.filename.as_deref(),
    };
    let from = match body.from.as_deref() {
        Some(from) => Some(format_id(from).ok_or_else(|| {
            ApiError::bad(
                "unsupported_format",
                &format!("`{from}` is not a format id. GET /v1/formats lists them."),
            )
        })?),
        None => source_name
            .and_then(routes::format_from_name)
            .or_else(|| body.content_type.as_deref().and_then(format_from_mime)),
    }
    .ok_or_else(|| {
        ApiError::bad(
            "input_format_required",
            "Could not tell the input format. Send `from`, such as from=png, or a file name with an extension.",
        )
    })?;
    let options = match body.options.take() {
        None => convt_core::Options::default(),
        Some(value) => serde_path_to_error::deserialize(value)
            .map_err(|e| ApiError::bad("invalid_options", &format!("options: {e}")))?,
    };
    let (data, bytes, extra) = match input {
        Input::Bytes(bytes) => {
            if bytes.is_empty() {
                return Err(ApiError::bad(
                    "file_too_large",
                    "The file is empty. Files must contain 1 byte to 25 MB here.",
                ));
            }
            if bytes.len() > MAX_INLINE_BYTES {
                return Err(too_large());
            }
            let digest = hex::encode(Sha256::digest(&bytes));
            (
                CreateJob {
                    input_format: from.into(),
                    target_format: target.into(),
                    input_bytes: Some(bytes.len() as i64),
                    options,
                    source_url: None,
                    webhook_url: None,
                },
                Some(bytes),
                digest,
            )
        }
        Input::Url(url) => (
            CreateJob {
                input_format: from.into(),
                target_format: target.into(),
                input_bytes: None,
                options,
                source_url: Some(url),
                webhook_url: None,
            },
            None,
            String::new(),
        ),
    };
    let has_key = routes::idempotency_key(&headers)?.is_some();
    let created = routes::create_job(&s, &who, &headers, &data, "convert", &extra).await?;
    let replayed = created.replayed;
    let mut job = created.job;
    let mut guard = CancelOnDrop(
        (!has_key && !replayed).then(|| (s.pool.clone(), who.clone(), job.id.clone())),
    );
    if job.status == "created"
        && let Some(bytes) = bytes
    {
        s.storage
            .put(job.input_key.as_deref().unwrap(), bytes)
            .await?;
        job = routes::queue_uploaded(&s, &who, job).await?;
    }
    let deadline = tokio::time::Instant::now() + Duration::from_secs(wait);
    let mut pause = Duration::from_millis(150);
    while !matches!(job.status.as_str(), "succeeded" | "failed" | "cancelled") {
        let now = tokio::time::Instant::now();
        // A draining server answers 202 rather than holding the request until it
        // is killed, so the client always learns the job id.
        if now >= deadline || routes::draining() {
            guard.disarm();
            let mut response = (
                StatusCode::ACCEPTED,
                Json(serde_json::json!({"job": JobView::from(job.clone())})),
            )
                .into_response();
            let h = response.headers_mut();
            if let Ok(location) = HeaderValue::from_str(&format!("/v1/jobs/{}", job.id)) {
                h.insert(header::LOCATION, location);
            }
            h.insert(header::RETRY_AFTER, HeaderValue::from_static("2"));
            return Ok(finish(response, &job.id, replayed));
        }
        tokio::time::sleep(pause.min(deadline - now)).await;
        pause = (pause * 2).min(Duration::from_millis(1000));
        job = jobs::owned(&s.pool, &who, &job.id).await?;
    }
    guard.disarm();
    let id = job.id.clone();
    match job.status.as_str() {
        "failed" => {
            let code = job
                .error_code
                .clone()
                .unwrap_or_else(|| "conversion_failed".into());
            let message = match code.as_str() {
                "source_fetch_failed" => {
                    "Could not download source_url. Check that it is public and returns the file."
                }
                "expired_or_exhausted" | "expired" => {
                    "The job ran out of time or attempts. Try again."
                }
                _ => {
                    "The conversion failed. Check the input file and options. You were not charged."
                }
            };
            return Ok(finish(
                job_error(StatusCode::UNPROCESSABLE_ENTITY, &code, message, job),
                &id,
                replayed,
            ));
        }
        "cancelled" => {
            return Ok(finish(
                job_error(
                    StatusCode::CONFLICT,
                    "cancelled",
                    "The job was cancelled before it finished. You were not charged.",
                    job,
                ),
                &id,
                replayed,
            ));
        }
        _ => {}
    }
    let wants_json = headers
        .get(header::ACCEPT)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|a| a.contains("application/json"));
    if wants_json || job.output_keys.0.len() != 1 {
        let (outputs, ttl) = routes::output_links(&s, &job).await?;
        let response = Json(serde_json::json!({
            "job": JobView::from(job),
            "outputs": outputs,
            "expires_in": ttl,
        }))
        .into_response();
        return Ok(finish(response, &id, replayed));
    }
    let key = job.output_keys.0[0].clone();
    let length = s.storage.head(&key).await?;
    let stream = s.storage.open(&key).await?;
    let mime = convt_core::format_by_id(&job.target_format)
        .map(|f| f.mime)
        .unwrap_or("application/octet-stream");
    let name = download_name(body.filename.as_deref().or(source_name_owned(&job)), &key);
    let mut response = Response::new(Body::from_stream(stream));
    let h = response.headers_mut();
    h.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_str(mime).unwrap_or(HeaderValue::from_static("application/octet-stream")),
    );
    if let Some(length) = length {
        h.insert(header::CONTENT_LENGTH, HeaderValue::from(length));
    }
    if let Ok(value) = HeaderValue::from_str(&format!("attachment; filename=\"{name}\"")) {
        h.insert(header::CONTENT_DISPOSITION, value);
    }
    Ok(finish(response, &id, replayed))
}

fn source_name_owned(job: &jobs::Job) -> Option<&str> {
    job.source_url
        .as_deref()
        .map(|u| u.split(['?', '#']).next().unwrap_or(u))
}

fn finish(mut response: Response, job_id: &str, replayed: bool) -> Response {
    let h = response.headers_mut();
    if let Ok(id) = HeaderValue::from_str(job_id) {
        h.insert("convt-job-id", id);
    }
    if replayed {
        h.insert("idempotent-replayed", HeaderValue::from_static("true"));
    }
    response
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_resolve_from_ids_extensions_and_mime() {
        assert_eq!(format_id("jpg"), Some("jpeg"));
        assert_eq!(format_id(".PNG"), Some("png"));
        assert_eq!(format_id("webp"), Some("webp"));
        assert_eq!(format_id("nope"), None);
        assert_eq!(format_from_mime("image/png; charset=binary"), Some("png"));
        assert_eq!(format_from_mime("application/octet-stream"), None);
    }

    #[test]
    fn download_names_are_safe() {
        assert_eq!(
            download_name(Some("photo.png"), "j/attempt-1/input.webp"),
            "photo.webp"
        );
        assert_eq!(
            download_name(Some("../../etc/pa\"ss\r\nwd.png"), "j/attempt-1/input.webp"),
            "passwd.webp"
        );
        assert_eq!(download_name(None, "j/attempt-1/input.pdf"), "output.pdf");
        assert_eq!(
            download_name(
                Some("https://cdn.example.com/a/report.docx"),
                "j/a/input.pdf"
            ),
            "report.pdf"
        );
    }
}
