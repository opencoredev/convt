use crate::{
    api_keys,
    jobs::{self, CreateJob, Idempotency, Principal},
    storage::Storage,
    tokens,
};
use axum::{
    Extension, Json, Router,
    extract::{Path, Request, State, rejection::JsonRejection},
    http::{HeaderMap, HeaderName, HeaderValue, StatusCode, header},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use serde::Serialize;
use sqlx::PgPool;
use std::sync::Arc;

static DRAINING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Called once on SIGTERM. Long requests answer early from then on.
pub fn start_draining() {
    DRAINING.store(true, std::sync::atomic::Ordering::SeqCst);
}
pub fn draining() -> bool {
    DRAINING.load(std::sync::atomic::Ordering::SeqCst)
}

/// Requests each key (or web session) may make per minute.
pub const RATE_LIMIT: i32 = 120;

#[derive(Clone)]
pub struct CloudState {
    pub pool: PgPool,
    pub storage: Arc<dyn Storage>,
    pub web_secret: String,
    /// `CONVT_WEBHOOK_SECRET`. Without it, `webhook_url` is refused.
    pub webhook_secret: Option<String>,
    /// `CONVT_SOURCE_URLS=1`. Off until every worker can download `source_url`
    /// inputs, because an older worker would fail those jobs.
    pub source_urls: bool,
}

tokio::task_local! {
    /// The `X-Request-Id` of the request being served, for error bodies and logs.
    pub static REQUEST_ID: String;
}

#[derive(Debug)]
pub struct ApiError {
    status: StatusCode,
    code: &'static str,
    message: String,
    headers: Vec<(HeaderName, HeaderValue)>,
    /// The job the failed request created or touched, so the caller can recover it.
    job: Option<Box<JobView>>,
    job_id: Option<String>,
}
impl ApiError {
    pub fn new(status: StatusCode, code: &'static str, message: &str) -> Self {
        Self {
            status,
            code,
            message: message.into(),
            headers: Vec::new(),
            job: None,
            job_id: None,
        }
    }
    pub fn bad(code: &'static str, message: &str) -> Self {
        Self::new(StatusCode::BAD_REQUEST, code, message)
    }
    pub fn forbidden(code: &'static str, message: &str) -> Self {
        Self::new(StatusCode::FORBIDDEN, code, message)
    }
    pub fn conflict(code: &'static str, message: &str) -> Self {
        Self::new(StatusCode::CONFLICT, code, message)
    }
    pub fn not_found(message: &str) -> Self {
        Self::new(StatusCode::NOT_FOUND, "not_found", message)
    }
    pub fn code(&self) -> &'static str {
        self.code
    }
    pub fn status(&self) -> StatusCode {
        self.status
    }
    pub fn with_job(mut self, job: jobs::Job) -> Self {
        self.job = Some(Box::new(job.into()));
        self
    }
    /// Names a job the caller can't be handed in full (the database is what
    /// failed): its id in the body and headers, and where to check it.
    pub fn with_job_id(mut self, id: &str) -> Self {
        if self.job.is_none() {
            self.job_id = Some(id.to_owned());
            self.message.push_str(&format!(
                " The job {id} exists and may still finish and be billed: check GET /v1/jobs/{id} before retrying."
            ));
            self = self.header(header::LOCATION, format!("/v1/jobs/{id}"));
        }
        self.header(HeaderName::from_static("convt-job-id"), id.to_owned())
    }
    pub fn with_header(self, name: HeaderName, value: String) -> Self {
        self.header(name, value)
    }
    fn header(mut self, name: HeaderName, value: String) -> Self {
        if let Ok(value) = HeaderValue::from_str(&value) {
            self.headers.push((name, value));
        }
        self
    }
}
impl From<sqlx::Error> for ApiError {
    fn from(e: sqlx::Error) -> Self {
        tracing::error!(error=%e,"database request failed");
        Self::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "internal",
            "Request failed. Retry shortly; include the request id if you contact support.",
        )
    }
}
impl From<anyhow::Error> for ApiError {
    fn from(e: anyhow::Error) -> Self {
        tracing::warn!(error=%e,"storage request failed");
        Self::new(
            StatusCode::BAD_GATEWAY,
            "storage_unavailable",
            "Object storage is unavailable. Retry shortly.",
        )
    }
}
impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let request_id = REQUEST_ID.try_with(Clone::clone).ok();
        let mut body = serde_json::json!({"error":{
            "code": self.code,
            "message": self.message,
            "request_id": request_id,
        }});
        if let Some(job) = self.job {
            body["job"] = serde_json::json!(job);
        }
        if let Some(id) = self.job_id {
            body["job_id"] = serde_json::json!(id);
        }
        let mut response = (self.status, Json(body)).into_response();
        response.headers_mut().extend(self.headers);
        response
    }
}

/// Rate-limit state for the current window, sent back as headers.
#[derive(Clone, Copy)]
struct RateWindow {
    remaining: i32,
    reset_seconds: i64,
}

async fn authenticate(
    state: &CloudState,
    headers: &HeaderMap,
) -> Result<(Principal, RateWindow), ApiError> {
    let token = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.strip_prefix("Bearer "))
        .ok_or_else(|| {
            ApiError::new(
                StatusCode::UNAUTHORIZED,
                "unauthorized",
                "Send your API key as Authorization: Bearer cvt_live_...",
            )
        })?;
    let who = if token.starts_with("cvt_web_") {
        Principal {
            user_id: tokens::verify(&state.web_secret, token).ok_or_else(|| {
                ApiError::forbidden("unauthorized", "Web credential expired or invalid.")
            })?,
            key_id: None,
        }
    } else {
        let owner = api_keys::find_by_key(&state.pool, token)
            .await?
            .ok_or_else(|| ApiError::forbidden("unauthorized", "API key is invalid or revoked."))?;
        api_keys::touch(&state.pool, &owner.key_id).await?;
        Principal {
            user_id: owner.user_id,
            key_id: Some(owner.key_id),
        }
    };
    let bucket = format!("cloud:{}", who.key_id.as_ref().unwrap_or(&who.user_id));
    let window = sqlx::query!(r#"insert into otp_send_limits (key,window_start,count,expires_at) values ($1,now(),1,now()+interval '1 minute') on conflict (key) do update set count=case when otp_send_limits.expires_at<=now() then 1 else otp_send_limits.count+1 end, expires_at=case when otp_send_limits.expires_at<=now() then now()+interval '1 minute' else otp_send_limits.expires_at end returning count, ceil(extract(epoch from expires_at-now()))::bigint as "reset!""#, bucket).fetch_one(&state.pool).await?;
    let rate = RateWindow {
        remaining: (RATE_LIMIT - window.count).max(0),
        reset_seconds: window.reset.max(1),
    };
    if window.count > RATE_LIMIT {
        return Err(rate_headers(
            ApiError::new(
                StatusCode::TOO_MANY_REQUESTS,
                "rate_limited",
                "Limit: 120 requests per minute per key. Wait for Retry-After seconds.",
            ),
            rate,
        )
        .header(header::RETRY_AFTER, rate.reset_seconds.to_string()));
    }
    Ok((who, rate))
}

fn rate_headers(error: ApiError, rate: RateWindow) -> ApiError {
    error
        .header(
            HeaderName::from_static("x-ratelimit-limit"),
            RATE_LIMIT.to_string(),
        )
        .header(
            HeaderName::from_static("x-ratelimit-remaining"),
            rate.remaining.to_string(),
        )
        .header(
            HeaderName::from_static("x-ratelimit-reset"),
            rate.reset_seconds.to_string(),
        )
}

/// Authenticates every jobs route once, counts it against the key's minute, and
/// puts the rate-limit headers on whatever the route answers.
async fn auth(State(s): State<CloudState>, mut request: Request, next: Next) -> Response {
    let (who, rate) = match authenticate(&s, request.headers()).await {
        Ok(found) => found,
        Err(error) => return error.into_response(),
    };
    request.extensions_mut().insert(who);
    let mut response = next.run(request).await;
    let headers = response.headers_mut();
    headers.insert("x-ratelimit-limit", HeaderValue::from(RATE_LIMIT));
    headers.insert("x-ratelimit-remaining", HeaderValue::from(rate.remaining));
    headers.insert("x-ratelimit-reset", HeaderValue::from(rate.reset_seconds));
    response
}

#[derive(Serialize, Debug)]
pub struct JobView {
    pub id: String,
    pub status: String,
    pub input_format: String,
    pub target_format: String,
    pub options: convt_core::Options,
    pub input_bytes: Option<i64>,
    pub attempt: i32,
    pub error_code: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub expires_at: chrono::DateTime<chrono::Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub webhook_url: Option<String>,
}
impl From<jobs::Job> for JobView {
    fn from(j: jobs::Job) -> Self {
        Self {
            id: j.id,
            status: j.status,
            input_format: j.input_format,
            target_format: j.target_format,
            options: j.options.0,
            input_bytes: j.input_bytes,
            attempt: j.attempt,
            error_code: j.error_code,
            created_at: j.created_at,
            expires_at: j.expires_at,
            source_url: j.source_url.as_deref().map(redact_url),
            webhook_url: j.webhook_url.as_deref().map(redact_url),
        }
    }
}

/// Only a URL's origin: paths, query strings and fragments can all carry
/// presigned signatures or receivers' tokens. Job views show this; the full URL
/// stays server-side and is cleared at expiry.
pub fn redact_url(url: &str) -> String {
    match url::Url::parse(url) {
        Ok(parsed) => {
            let origin = parsed.origin().ascii_serialization();
            let has_more =
                parsed.path() != "/" || parsed.query().is_some() || parsed.fragment().is_some();
            if has_more {
                format!("{origin}/…")
            } else {
                origin
            }
        }
        Err(_) => "[unparseable URL]".into(),
    }
}

/// The `{id}` path segment, with the JSON error shape when it can't be decoded.
pub struct JobId(pub String);
impl<S: Send + Sync> axum::extract::FromRequestParts<S> for JobId {
    type Rejection = ApiError;
    async fn from_request_parts(
        parts: &mut axum::http::request::Parts,
        state: &S,
    ) -> Result<Self, Self::Rejection> {
        Path::<String>::from_request_parts(parts, state)
            .await
            .map(|Path(id)| JobId(id))
            .map_err(|_| ApiError::not_found("Job not found."))
    }
}

/// Reads `Idempotency-Key`: 1 to 255 visible ASCII characters.
pub fn idempotency_key(headers: &HeaderMap) -> Result<Option<String>, ApiError> {
    let Some(value) = headers.get("idempotency-key") else {
        return Ok(None);
    };
    let key = value
        .to_str()
        .ok()
        .filter(|k| !k.is_empty() && k.len() <= 255 && k.bytes().all(|b| b.is_ascii_graphic()))
        .ok_or_else(|| {
            ApiError::bad(
                "invalid_idempotency_key",
                "Idempotency-Key must be 1 to 255 visible ASCII characters, such as a UUID.",
            )
        })?;
    Ok(Some(key.to_owned()))
}

/// The format a URL's or file name's extension names, if any.
pub fn format_from_name(name: &str) -> Option<&'static str> {
    let path = name.split(['?', '#']).next().unwrap_or(name);
    let file = path.rsplit('/').next().unwrap_or(path);
    let (_, extension) = file.rsplit_once('.')?;
    let extension = extension.to_ascii_lowercase();
    convt_core::FORMATS
        .iter()
        .find(|f| f.extensions.contains(&extension.as_str()))
        .map(|f| f.id)
}

/// Reads a job request so every malformed body gets the JSON error shape:
/// `invalid_options` when only `options` is wrong, the same code an
/// out-of-range value gets, and `invalid_request` for anything else. A
/// `source_url` request may leave out `input_format` when the URL's path ends in
/// a known extension.
fn parse_create(
    body: Result<Json<serde_json::Value>, JsonRejection>,
) -> Result<CreateJob, ApiError> {
    let Json(mut body) = body.map_err(|e| ApiError::bad("invalid_request", &e.body_text()))?;
    let options = body.as_object_mut().and_then(|o| o.remove("options"));
    if let Some(object) = body.as_object_mut()
        && !object.contains_key("input_format")
        && let Some(format) = object
            .get("source_url")
            .and_then(|u| u.as_str())
            .and_then(format_from_name)
    {
        object.insert("input_format".into(), format.into());
    }
    let mut data: CreateJob = serde_path_to_error::deserialize(body)
        .map_err(|e| ApiError::bad("invalid_request", &e.to_string()))?;
    if let Some(options) = options {
        data.options = serde_path_to_error::deserialize(options)
            .map_err(|e| ApiError::bad("invalid_options", &format!("options: {e}")))?;
    }
    Ok(data)
}

/// Creates a job, honoring `Idempotency-Key`. `extra` joins the fingerprint, so
/// a key reused with different file bytes is refused.
pub async fn create_job(
    s: &CloudState,
    who: &Principal,
    headers: &HeaderMap,
    data: &CreateJob,
    route: &str,
    extra: &str,
) -> Result<jobs::Created, ApiError> {
    let idempotency = idempotency_key(headers)?.map(|key| Idempotency {
        request_hash: data.fingerprint(route, extra),
        key,
    });
    let features = jobs::Features {
        source_urls: s.source_urls,
        webhooks: s.webhook_secret.is_some(),
    };
    jobs::create_with(&s.pool, who, data, idempotency.as_ref(), features).await
}

async fn create(
    State(s): State<CloudState>,
    Extension(who): Extension<Principal>,
    h: HeaderMap,
    body: Result<Json<serde_json::Value>, JsonRejection>,
) -> Result<Response, ApiError> {
    let data = parse_create(body)?;
    let created = create_job(&s, &who, &h, &data, "jobs", "").await?;
    let job = created.job;
    // Only a job still waiting for its upload gets a URL; a replay of a started
    // job must not hand out a way to write its input again.
    // Only a job still waiting for its upload gets a URL; a replay of a started
    // job must not hand out a way to write its input again. The URL never
    // outlives the job, so nothing can land in storage after expiry cleanup.
    let upload_ttl = (job.expires_at - chrono::Utc::now()).num_seconds() - 60;
    let upload_url = if job.status == "created" && job.source_url.is_none() && upload_ttl >= 60 {
        let seconds = upload_ttl.min(900) as u32;
        match s
            .storage
            .upload_url(
                job.input_key.as_deref().unwrap(),
                job.input_bytes.unwrap(),
                seconds,
            )
            .await
        {
            Ok(url) => Some((url, seconds)),
            Err(e) => {
                if !created.replayed {
                    let _ = jobs::cancel(&s.pool, &who, &job.id).await;
                }
                return Err(e.into());
            }
        }
    } else {
        None
    };
    let mut response = Json(serde_json::json!({
        "job": JobView::from(job),
        "upload_url": upload_url.as_ref().map(|(url, _)| url),
        "upload_expires_in": upload_url.as_ref().map(|(_, seconds)| seconds),
    }))
    .into_response();
    if created.replayed {
        response
            .headers_mut()
            .insert("idempotent-replayed", HeaderValue::from_static("true"));
    }
    Ok(response)
}

async fn status(
    State(s): State<CloudState>,
    Extension(who): Extension<Principal>,
    JobId(id): JobId,
) -> Result<Json<JobView>, ApiError> {
    Ok(Json(jobs::owned(&s.pool, &who, &id).await?.into()))
}

/// Seals a `created` job's upload, checks its size and queues it. Any other job
/// comes back unchanged, so calling this twice is safe.
pub async fn queue_uploaded(
    s: &CloudState,
    who: &Principal,
    job: jobs::Job,
) -> Result<jobs::Job, ApiError> {
    if job.status != "created" || job.source_url.is_some() {
        return Ok(job);
    }
    let id = job.id.clone();
    let mismatch = || {
        ApiError::bad(
            "size_mismatch",
            "Uploaded size must match the reservation and stay within 2 GB.",
        )
    };
    // Reject staging mismatches before paying to copy an oversized object. The
    // sealed snapshot below still gets its own check to close overwrite races.
    let Some(staging_bytes) = s.storage.head(job.input_key.as_deref().unwrap()).await? else {
        return Err(ApiError::conflict(
            "upload_missing",
            "Nothing was uploaded for this job yet. PUT the file to upload_url, then start it.",
        ));
    };
    if staging_bytes > jobs::MAX_FILE_BYTES || Some(staging_bytes) != job.input_bytes {
        let _ = jobs::cancel(&s.pool, who, &id).await?;
        return Err(mismatch());
    }
    // Unique destination per start prevents two simultaneous starts from mutating
    // the input that the winner queued. Check the sealed snapshot, not staging.
    let sealed = format!("{id}/input-{}", crate::ids::new_id("seal"));
    s.storage
        .seal(job.input_key.as_deref().unwrap(), &sealed)
        .await?;
    let bytes = s.storage.size(&sealed).await?;
    if bytes > jobs::MAX_FILE_BYTES || Some(bytes) != job.input_bytes {
        s.storage.delete(&sealed).await?;
        let _ = jobs::cancel(&s.pool, who, &id).await?;
        return Err(mismatch());
    }
    let changed=sqlx::query!(r#"update cloud_jobs set status='queued',input_key=$3,queued_at=now(),updated_at=now() where id=$1 and user_id=$2 and status='created' and expires_at>now() and reservation='open'"#, &id, &who.user_id, &sealed).execute(&s.pool).await?.rows_affected();
    if changed == 0 {
        s.storage.delete(&sealed).await?;
    }
    jobs::owned(&s.pool, who, &id).await
}

async fn start(
    State(s): State<CloudState>,
    Extension(who): Extension<Principal>,
    JobId(id): JobId,
) -> Result<Json<JobView>, ApiError> {
    let job = jobs::owned(&s.pool, &who, &id).await?;
    Ok(Json(queue_uploaded(&s, &who, job).await?.into()))
}
async fn cancel(
    State(s): State<CloudState>,
    Extension(who): Extension<Principal>,
    JobId(id): JobId,
) -> Result<Json<JobView>, ApiError> {
    Ok(Json(jobs::cancel(&s.pool, &who, &id).await?.into()))
}

/// Signed URLs for a succeeded job's outputs, valid for up to five minutes.
pub async fn output_links(
    s: &CloudState,
    job: &jobs::Job,
) -> Result<(Vec<serde_json::Value>, u32), ApiError> {
    let ttl = (job.expires_at - chrono::Utc::now())
        .num_seconds()
        .clamp(1, 300) as u32;
    let mut outputs = Vec::new();
    for key in &job.output_keys.0 {
        outputs.push(serde_json::json!({"name":key.rsplit('/').next().unwrap_or("output"),"url":s.storage.download_url(key,ttl).await?}));
    }
    Ok((outputs, ttl))
}

async fn download(
    State(s): State<CloudState>,
    Extension(who): Extension<Principal>,
    JobId(id): JobId,
) -> Result<Json<serde_json::Value>, ApiError> {
    let job = jobs::owned(&s.pool, &who, &id).await?;
    if job.status != "succeeded" {
        return Err(ApiError::bad(
            "not_ready",
            "Job has no completed outputs. Wait until its status is succeeded.",
        ));
    }
    let (outputs, ttl) = output_links(&s, &job).await?;
    Ok(Json(
        serde_json::json!({"outputs":outputs,"expires_in":ttl}),
    ))
}

/// The webhook secret is a long-lived account credential: only an API key may
/// read or rotate it, never the five-minute web credential the browser converter
/// holds, and responses are never cached.
fn secret_response(master: &str, who: &Principal, version: i32) -> Response {
    let mut response = Json(serde_json::json!({
        "secret": crate::webhooks::account_secret(master, &who.user_id, version),
        "version": version,
    }))
    .into_response();
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}
fn webhook_master<'a>(s: &'a CloudState, who: &Principal) -> Result<&'a str, ApiError> {
    if who.key_id.is_none() {
        return Err(ApiError::forbidden(
            "api_key_required",
            "Use an API key to read or rotate the webhook secret.",
        ));
    }
    s.webhook_secret.as_deref().ok_or_else(|| {
        ApiError::bad(
            "webhooks_unavailable",
            "Webhooks are not enabled on this server yet.",
        )
    })
}
async fn webhook_secret(
    State(s): State<CloudState>,
    Extension(who): Extension<Principal>,
) -> Result<Response, ApiError> {
    let master = webhook_master(&s, &who)?;
    let version = crate::webhooks::secret_version(&s.pool, &who.user_id).await?;
    Ok(secret_response(master, &who, version))
}
async fn rotate_webhook_secret(
    State(s): State<CloudState>,
    Extension(who): Extension<Principal>,
) -> Result<Response, ApiError> {
    let master = webhook_master(&s, &who)?;
    let version = crate::webhooks::rotate(&s.pool, &who.user_id).await?;
    Ok(secret_response(master, &who, version))
}

pub fn router(state: CloudState) -> Router {
    Router::new()
        .route("/v1/jobs", post(create))
        .route("/v1/jobs/{id}", get(status))
        .route("/v1/jobs/{id}/start", post(start))
        .route("/v1/jobs/{id}/cancel", post(cancel))
        .route("/v1/jobs/{id}/download", get(download))
        .route("/v1/webhooks/secret", get(webhook_secret))
        .route("/v1/webhooks/secret/rotate", post(rotate_webhook_secret))
        .merge(crate::convert::router())
        .route_layer(middleware::from_fn_with_state(state.clone(), auth))
        .layer(tower_http::cors::CorsLayer::permissive())
        .with_state(state)
}
/// `GET /ready`: 200 when the database and object storage answer, else 503.
/// `/health` stays a liveness check that needs neither.
async fn ready(State(s): State<CloudState>) -> Response {
    let limit = std::time::Duration::from_secs(3);
    let database = tokio::time::timeout(limit, sqlx::query_scalar!("select 1").fetch_one(&s.pool))
        .await
        .is_ok_and(|r| r.is_ok());
    let storage = tokio::time::timeout(limit, s.storage.check())
        .await
        .is_ok_and(|r| r.is_ok());
    let ok = database && storage && !draining();
    (
        if ok {
            StatusCode::OK
        } else {
            StatusCode::SERVICE_UNAVAILABLE
        },
        Json(serde_json::json!({
            "status": if ok { "ready" } else { "unavailable" },
            "database": database,
            "storage": storage,
            "draining": draining(),
        })),
    )
        .into_response()
}

pub fn ready_router(state: CloudState) -> Router {
    Router::new().route("/ready", get(ready)).with_state(state)
}

pub async fn expire(s: &CloudState) -> anyhow::Result<()> {
    // Only rows that still hold something: an open reservation or an unfinished
    // status. Finished rows keep their status, so a cancelled job stays cancelled.
    sqlx::query!(r#"update cloud_jobs set status=case when status in ('succeeded','failed','cancelled') then status else 'failed' end,reservation=case when reservation='open' then 'released' else reservation end,lease_expires_at=null,error_code=case when reservation='open' then 'expired' else error_code end,updated_at=now() where expires_at<=now() and (reservation='open' or status not in ('succeeded','failed','cancelled'))"#).execute(&s.pool).await?;
    // Customer URLs can be credentials. Drop them at expiry, whether or not file
    // cleanup succeeds; a webhook URL stays until its delivery is done or given up,
    // which takes under three hours.
    sqlx::query!(r#"update cloud_jobs set source_url=null, webhook_url=case when webhook_delivered_at is not null or webhook_attempts>=$1 or expires_at<=now()-interval '6 hours' then null else webhook_url end, updated_at=now() where expires_at<=now() and (source_url is not null or (webhook_url is not null and (webhook_delivered_at is not null or webhook_attempts>=$1 or expires_at<=now()-interval '6 hours')))"#, crate::webhooks::MAX_ATTEMPTS).execute(&s.pool).await?;
    let ids: Vec<String> = sqlx::query_scalar!(
        r#"select id from cloud_jobs where expires_at<=now() and input_key is not null"#
    )
    .fetch_all(&s.pool)
    .await?;
    for id in ids {
        s.storage.delete_prefix(&format!("{id}/")).await?;
        // Only successful cleanup releases the separate storage commitment.
        sqlx::query!(
            r#"update cloud_jobs set input_key=null where id=$1 and expires_at<=now()"#,
            &id
        )
        .execute(&s.pool)
        .await?;
    }
    Ok(())
}
