use crate::{
    api_keys,
    jobs::{self, CreateJob, Principal},
    storage::Storage,
    tokens,
};
use axum::{
    Json, Router,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use serde::Serialize;
use sqlx::PgPool;
use std::sync::Arc;
#[derive(Clone)]
pub struct CloudState {
    pub pool: PgPool,
    pub storage: Arc<dyn Storage>,
    pub web_secret: String,
}
#[derive(Debug)]
pub struct ApiError {
    status: StatusCode,
    code: &'static str,
    message: String,
}
impl ApiError {
    pub fn bad(code: &'static str, message: &str) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            code,
            message: message.into(),
        }
    }
    pub fn forbidden(code: &'static str, message: &str) -> Self {
        Self {
            status: StatusCode::FORBIDDEN,
            code,
            message: message.into(),
        }
    }
    pub fn not_found(message: &str) -> Self {
        Self {
            status: StatusCode::NOT_FOUND,
            code: "not_found",
            message: message.into(),
        }
    }
}
impl From<sqlx::Error> for ApiError {
    fn from(e: sqlx::Error) -> Self {
        tracing::error!(error=%e,"database request failed");
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            code: "internal",
            message: "Request failed.".into(),
        }
    }
}
impl From<anyhow::Error> for ApiError {
    fn from(e: anyhow::Error) -> Self {
        tracing::warn!(error=%e,"storage request failed");
        Self {
            status: StatusCode::BAD_GATEWAY,
            code: "storage_unavailable",
            message: "Object storage is unavailable. Retry shortly.".into(),
        }
    }
}
impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (
            self.status,
            Json(serde_json::json!({"error":{"code":self.code,"message":self.message}})),
        )
            .into_response()
    }
}
async fn authenticate(state: &CloudState, headers: &HeaderMap) -> Result<Principal, ApiError> {
    let token = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.strip_prefix("Bearer "))
        .ok_or_else(|| ApiError {
            status: StatusCode::UNAUTHORIZED,
            code: "unauthorized",
            message: "A Bearer credential is required.".into(),
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
    let count:i32=sqlx::query_scalar!(r#"insert into otp_send_limits (key,window_start,count,expires_at) values ($1,now(),1,now()+interval '1 minute') on conflict (key) do update set count=case when otp_send_limits.expires_at<=now() then 1 else otp_send_limits.count+1 end, expires_at=case when otp_send_limits.expires_at<=now() then now()+interval '1 minute' else otp_send_limits.expires_at end returning count"#, bucket).fetch_one(&state.pool).await?;
    if count > 120 {
        return Err(ApiError {
            status: StatusCode::TOO_MANY_REQUESTS,
            code: "rate_limited",
            message: "Limit: 120 requests per minute per key.".into(),
        });
    }
    Ok(who)
}
#[derive(Serialize)]
pub struct JobView {
    pub id: String,
    pub status: String,
    pub input_format: String,
    pub target_format: String,
    pub input_bytes: Option<i64>,
    pub attempt: i32,
    pub error_code: Option<String>,
    pub expires_at: chrono::DateTime<chrono::Utc>,
}
impl From<jobs::Job> for JobView {
    fn from(j: jobs::Job) -> Self {
        Self {
            id: j.id,
            status: j.status,
            input_format: j.input_format,
            target_format: j.target_format,
            input_bytes: j.input_bytes,
            attempt: j.attempt,
            error_code: j.error_code,
            expires_at: j.expires_at,
        }
    }
}
async fn create(
    State(s): State<CloudState>,
    h: HeaderMap,
    Json(data): Json<CreateJob>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let who = authenticate(&s, &h).await?;
    let job = jobs::create(&s.pool, &who, &data).await?;
    let url = match s
        .storage
        .upload_url(job.input_key.as_deref().unwrap(), job.input_bytes.unwrap())
        .await
    {
        Ok(url) => url,
        Err(e) => {
            let _ = jobs::cancel(&s.pool, &who, &job.id).await;
            return Err(e.into());
        }
    };
    Ok(Json(
        serde_json::json!({"job":JobView::from(job),"upload_url":url,"upload_expires_in":900}),
    ))
}
async fn status(
    State(s): State<CloudState>,
    h: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<JobView>, ApiError> {
    let who = authenticate(&s, &h).await?;
    Ok(Json(jobs::owned(&s.pool, &who, &id).await?.into()))
}
async fn start(
    State(s): State<CloudState>,
    h: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<JobView>, ApiError> {
    let who = authenticate(&s, &h).await?;
    let job = jobs::owned(&s.pool, &who, &id).await?;
    if job.status != "created" {
        return Ok(Json(job.into()));
    }
    // Reject staging mismatches before paying to copy an oversized object. The
    // sealed snapshot below still gets its own check to close overwrite races.
    let staging_bytes = s.storage.size(job.input_key.as_deref().unwrap()).await?;
    if staging_bytes > jobs::MAX_FILE_BYTES || Some(staging_bytes) != job.input_bytes {
        let _ = jobs::cancel(&s.pool, &who, &id).await?;
        return Err(ApiError::bad(
            "size_mismatch",
            "Uploaded size must match the reservation and stay within 2 GB.",
        ));
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
        let _ = jobs::cancel(&s.pool, &who, &id).await?;
        return Err(ApiError::bad(
            "size_mismatch",
            "Uploaded size must match the reservation and stay within 2 GB.",
        ));
    }
    let changed=sqlx::query!(r#"update cloud_jobs set status='queued',input_key=$3,queued_at=now(),updated_at=now() where id=$1 and user_id=$2 and status='created' and expires_at>now() and reservation='open'"#, &id, &who.user_id, &sealed).execute(&s.pool).await?.rows_affected();
    if changed == 0 {
        s.storage.delete(&sealed).await?;
    }
    Ok(Json(jobs::owned(&s.pool, &who, &id).await?.into()))
}
async fn cancel(
    State(s): State<CloudState>,
    h: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<JobView>, ApiError> {
    let who = authenticate(&s, &h).await?;
    Ok(Json(jobs::cancel(&s.pool, &who, &id).await?.into()))
}
async fn download(
    State(s): State<CloudState>,
    h: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let who = authenticate(&s, &h).await?;
    let job = jobs::owned(&s.pool, &who, &id).await?;
    if job.status != "succeeded" {
        return Err(ApiError::bad("not_ready", "Job has no completed outputs."));
    }
    let ttl = (job.expires_at - chrono::Utc::now())
        .num_seconds()
        .clamp(1, 300) as u32;
    let mut outputs = Vec::new();
    for key in &job.output_keys.0 {
        outputs.push(serde_json::json!({"name":key.rsplit('/').next().unwrap_or("output"),"url":s.storage.download_url(key,ttl).await?}));
    }
    Ok(Json(
        serde_json::json!({"outputs":outputs,"expires_in":ttl}),
    ))
}
pub fn router(state: CloudState) -> Router {
    Router::new()
        .route("/v1/jobs", post(create))
        .route("/v1/jobs/{id}", get(status))
        .route("/v1/jobs/{id}/start", post(start))
        .route("/v1/jobs/{id}/cancel", post(cancel))
        .route("/v1/jobs/{id}/download", get(download))
        .layer(tower_http::cors::CorsLayer::permissive())
        .with_state(state)
}
pub async fn expire(s: &CloudState) -> anyhow::Result<()> {
    sqlx::query!(r#"update cloud_jobs set status=case when status='succeeded' then status else 'failed' end,reservation=case when reservation='open' then 'released' else reservation end,lease_expires_at=null,error_code=case when reservation='open' then 'expired' else error_code end where expires_at<=now()"#).execute(&s.pool).await?;
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
