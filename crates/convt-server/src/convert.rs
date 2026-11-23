//! `POST /v1/convert`: one request in, the converted file out.
//!
//! It runs the same pipeline as the jobs routes: the same reservation and spend
//! cap, the same sealed upload and size check, the same queue, workers and
//! metering. It only saves the client the upload, start, poll and download calls.
//!
//! The input is a multipart `file` field, a raw request body, or a JSON body with
//! `source_url`. When the job succeeds with one output the response is that file;
//! with several outputs, or `Accept: application/json`, it is JSON with signed
//! URLs.
//!
//! A job still running after `wait` seconds: a JSON caller gets `202` and the job
//! keeps running. A file caller with an `Idempotency-Key` gets `409 still_running`
//! and the job keeps running, so a retry with the key collects it. A file caller
//! without a key gets `504 timeout` and the job is cancelled, because nothing else
//! could ever fetch its result.
//!
//! Without an `Idempotency-Key`, hanging up before the answer cancels the job. That
//! is best effort: a job that already succeeded is billed.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
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
/// Largest form field or JSON body other than the file itself.
const MAX_METADATA_BYTES: usize = 16 * 1024;

/// Request bodies held in memory at once across this process, in MB. Each inline
/// request holds its share from before the body is read until the bytes are
/// stored, so many large uploads at once queue instead of exhausting memory.
fn inline_budget() -> &'static tokio::sync::Semaphore {
    static BUDGET: std::sync::OnceLock<tokio::sync::Semaphore> = std::sync::OnceLock::new();
    BUDGET.get_or_init(|| {
        let mb = std::env::var("CONVT_INLINE_BUDGET_MB")
            .ok()
            .and_then(|v| v.parse::<usize>().ok())
            .unwrap_or(512)
            .max(32);
        tokio::sync::Semaphore::new(mb)
    })
}

async fn reserve_memory(
    headers: &HeaderMap,
) -> Result<tokio::sync::SemaphorePermit<'static>, ApiError> {
    let declared = headers
        .get(header::CONTENT_LENGTH)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<usize>().ok());
    if declared.is_some_and(|n| n > MAX_INLINE_BYTES + 64 * 1024) {
        return Err(too_large());
    }
    let mb = declared.map_or(26, |n| n.div_ceil(1_000_000).clamp(1, 26)) as u32;
    match tokio::time::timeout(Duration::from_secs(10), inline_budget().acquire_many(mb)).await {
        Ok(Ok(permit)) => Ok(permit),
        _ => Err(ApiError::new(
            StatusCode::SERVICE_UNAVAILABLE,
            "server_busy",
            "Too many uploads are in progress. Retry in a few seconds.",
        )
        .with_header(header::RETRY_AFTER, "5".into())),
    }
}

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
                let mut field = match form.next_field().await {
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
                        // Metadata is small; cap it before it is decoded so the
                        // memory budget, sized for file bytes, stays honest.
                        let mut text = Vec::new();
                        while let Some(chunk) = field
                            .chunk()
                            .await
                            .map_err(|e| ApiError::bad("invalid_request", &e.body_text()))?
                        {
                            text.extend_from_slice(&chunk);
                            if text.len() > MAX_METADATA_BYTES {
                                return Err(ApiError::bad(
                                    "invalid_request",
                                    &format!("The `{name}` field is over 16 KB."),
                                ));
                            }
                        }
                        let text = String::from_utf8(text).map_err(|_| {
                            ApiError::bad("invalid_request", "Form fields must be UTF-8.")
                        })?;
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
            if request
                .headers()
                .get(header::CONTENT_LENGTH)
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.parse::<usize>().ok())
                .is_none_or(|n| n > MAX_METADATA_BYTES)
            {
                return Err(ApiError::bad(
                    "invalid_request",
                    "A JSON body must declare its Content-Length and stay under 16 KB.",
                ));
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

/// Cancels the job when the client hangs up before the answer. The job id is
/// registered under the same lock that records the hang-up, from a task that
/// outlives the request, so a disconnect during job creation still cancels the
/// job the creation commits. Best effort: a job that already succeeded stays billed.
#[derive(Default)]
struct Hangup {
    gone: bool,
    armed: bool,
    job: Option<String>,
}
struct HangupGuard {
    state: Arc<Mutex<Hangup>>,
    pool: sqlx::PgPool,
    who: Principal,
}
fn cancel_later(pool: sqlx::PgPool, who: Principal, id: String) {
    if let Ok(handle) = tokio::runtime::Handle::try_current() {
        handle.spawn(async move {
            if let Err(error) = jobs::cancel(&pool, &who, &id).await {
                tracing::warn!(
                    job_id = id,
                    error = error.code(),
                    "cancel after hang-up failed"
                );
            }
        });
    }
}
impl HangupGuard {
    fn new(pool: sqlx::PgPool, who: Principal, armed: bool) -> Self {
        Self {
            state: Arc::new(Mutex::new(Hangup {
                armed,
                ..Default::default()
            })),
            pool,
            who,
        }
    }
    /// Called by the creating task once the job is committed.
    fn register(state: &Mutex<Hangup>, pool: &sqlx::PgPool, who: &Principal, id: &str) {
        let mut hangup = state.lock().unwrap_or_else(|e| e.into_inner());
        if hangup.gone && hangup.armed {
            cancel_later(pool.clone(), who.clone(), id.to_owned());
        } else {
            hangup.job = Some(id.to_owned());
        }
    }
    fn disarm(&self) {
        self.state.lock().unwrap_or_else(|e| e.into_inner()).armed = false;
    }
}
impl Drop for HangupGuard {
    fn drop(&mut self) {
        let mut hangup = self.state.lock().unwrap_or_else(|e| e.into_inner());
        hangup.gone = true;
        if hangup.armed
            && let Some(id) = hangup.job.take()
        {
            cancel_later(self.pool.clone(), self.who.clone(), id);
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
    let _memory = reserve_memory(&headers).await?;
    // A slow sender must not hold its share of the memory budget forever.
    let mut body = tokio::time::timeout(Duration::from_secs(120), read_body(request, &s))
        .await
        .map_err(|_| {
            ApiError::new(
                StatusCode::REQUEST_TIMEOUT,
                "request_timeout",
                "The upload took longer than 2 minutes. Use POST /v1/jobs for slow or large uploads.",
            )
        })??;
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
                    input_sha256: None,
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
                input_sha256: None,
            },
            None,
            String::new(),
        ),
    };
    let has_key = routes::idempotency_key(&headers)?.is_some();
    let guard = HangupGuard::new(s.pool.clone(), who.clone(), !has_key);
    // Creation runs in its own task so a hang-up mid-transaction cannot leave a
    // committed job that nobody cancels.
    let created = {
        let (s, who, headers, state) =
            (s.clone(), who.clone(), headers.clone(), guard.state.clone());
        tokio::spawn(async move {
            let created = routes::create_job(&s, &who, &headers, &data, "convert", &extra).await;
            if let Ok(created) = &created
                && !created.replayed
            {
                HangupGuard::register(&state, &s.pool, &who, &created.job.id);
            }
            created
        })
        .await
        .map_err(|_| {
            ApiError::new(
                StatusCode::INTERNAL_SERVER_ERROR,
                "internal",
                "Request failed.",
            )
        })??
    };
    let replayed = created.replayed;
    let mut job = created.job;
    let job_id = job.id.clone();
    // Any error from here on concerns a job that exists and may still run and
    // bill, so every answer names it and says where to look before retrying.
    let outcome: Result<Response, ApiError> = async {
    if job.status == "created"
        && let Some(bytes) = bytes
    {
        let stored = async {
            s.storage
                .put(job.input_key.as_deref().unwrap(), bytes)
                .await?;
            routes::queue_uploaded(&s, &who, job.clone()).await
        }
        .await;
        job = match stored {
            Ok(job) => job,
            // Without a key the guard cancels the job as this returns; with one, a
            // retry under the key uploads again. Either way the caller learns the id.
            Err(error) => {
                let id = job.id.clone();
                let job = jobs::owned(&s.pool, &who, &id).await.unwrap_or(job);
                return Ok(finish(error.with_job(job).into_response(), &id, replayed));
            }
        };
    } else {
        drop(bytes);
    }
    drop(_memory);
    let wants_json = wants_json(&headers);
    let deadline = tokio::time::Instant::now() + Duration::from_secs(wait);
    let mut pause = Duration::from_millis(150);
    while !matches!(job.status.as_str(), "succeeded" | "failed" | "cancelled") {
        let now = tokio::time::Instant::now();
        // A draining server answers now rather than holding the request until it
        // is killed, so the client always learns what happened to the job.
        if now >= deadline || routes::draining() {
            guard.disarm();
            if wants_json || has_key {
                let (status, body) = if wants_json {
                    (
                        StatusCode::ACCEPTED,
                        serde_json::json!({"job": JobView::from(job.clone())}),
                    )
                } else {
                    (
                        StatusCode::CONFLICT,
                        serde_json::json!({
                            "error": {"code": "still_running", "message": "The job is still running. Retry with the same Idempotency-Key, or poll the job.", "request_id": routes::REQUEST_ID.try_with(Clone::clone).ok()},
                            "job": JobView::from(job.clone()),
                        }),
                    )
                };
                let mut response = (status, Json(body)).into_response();
                let h = response.headers_mut();
                if let Ok(location) = HeaderValue::from_str(&format!("/v1/jobs/{}", job.id)) {
                    h.insert(header::LOCATION, location);
                }
                h.insert(header::RETRY_AFTER, HeaderValue::from_static("2"));
                return Ok(finish(response, &job.id, replayed));
            }
            // A file caller without a key could never fetch this job's result, so
            // stop it. If it finished in the meantime, serve it after all.
            job = jobs::cancel(&s.pool, &who, &job.id).await?;
            if job.status != "succeeded" {
                return Ok(finish(
                    job_error(
                        StatusCode::GATEWAY_TIMEOUT,
                        "timeout",
                        "The conversion did not finish within `wait` seconds and was cancelled; you were not charged. Raise `wait` (up to 300), send an Idempotency-Key, or use the jobs routes.",
                        job.clone(),
                    ),
                    &job.id,
                    replayed,
                ));
            }
            break;
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
    if wants_json || job.output_keys.0.len() != 1 {
        let (outputs, ttl) = match routes::output_links(&s, &job).await {
            Ok(links) => links,
            Err(error) => return Ok(finish(billed_error(error, job), &id, replayed)),
        };
        let response = Json(serde_json::json!({
            "job": JobView::from(job),
            "outputs": outputs,
            "expires_in": ttl,
        }))
        .into_response();
        return Ok(finish(response, &id, replayed));
    }
    let key = job.output_keys.0[0].clone();
    let opened = async {
        let length = s.storage.head(&key).await?;
        let stream = s.storage.open(&key).await?;
        Ok::<_, ApiError>((length, stream))
    }
    .await;
    let (length, stream) = match opened {
        Ok(opened) => opened,
        Err(error) => return Ok(finish(billed_error(error, job), &id, replayed)),
    };
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
    .await;
    Ok(outcome.unwrap_or_else(|error| {
        finish(
            error.with_job_id(&job_id).into_response(),
            &job_id,
            replayed,
        )
    }))
}

/// A storage error after the job succeeded: it was billed, so the answer must
/// name the job, whose outputs `GET /v1/jobs/{id}/download` still returns.
fn billed_error(error: ApiError, job: jobs::Job) -> Response {
    let id = job.id.clone();
    let mut response = error.with_job(job).into_response();
    if let Ok(location) = HeaderValue::from_str(&format!("/v1/jobs/{id}/download")) {
        response.headers_mut().insert(header::LOCATION, location);
    }
    response
}

/// Whether `Accept` asks for JSON: an `application/json` range with a nonzero
/// quality. `*/*` alone (curl, browsers) means the file.
fn wants_json(headers: &HeaderMap) -> bool {
    headers
        .get_all(header::ACCEPT)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|v| v.split(','))
        .any(|range| {
            let mut parts = range.split(';').map(str::trim);
            let media = parts.next().unwrap_or_default().to_ascii_lowercase();
            let quality = parts
                .find_map(|p| p.strip_prefix("q=").or_else(|| p.strip_prefix("Q=")))
                .and_then(|q| q.parse::<f32>().ok())
                .unwrap_or(1.0);
            media == "application/json" && quality > 0.0
        })
}

fn source_name_owned(job: &jobs::Job) -> Option<&str> {
    job.source_url
        .as_deref()
        .map(|u| u.split(['?', '#']).next().unwrap_or(u))
}

fn finish(mut response: Response, job_id: &str, replayed: bool) -> Response {
    let h = response.headers_mut();
    if !job_id.is_empty()
        && let Ok(id) = HeaderValue::from_str(job_id)
    {
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
    fn accept_needs_a_nonzero_json_range() {
        let with = |value: &str| {
            let mut h = HeaderMap::new();
            h.insert(header::ACCEPT, HeaderValue::from_str(value).unwrap());
            wants_json(&h)
        };
        assert!(with("application/json"));
        assert!(with("image/webp, application/json;q=0.5"));
        assert!(!with("image/webp, application/json;q=0"));
        assert!(!with("*/*"));
        assert!(!wants_json(&HeaderMap::new()));
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
