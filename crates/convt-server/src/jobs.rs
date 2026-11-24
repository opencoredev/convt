//! Transactional reservations and attempt-fenced Postgres queue.
use crate::{ids::new_id, routes::ApiError};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgPool};

pub const MAX_FILE_BYTES: i64 = 2_000_000_000;
pub const PRO_MONTH_BYTES: i64 = 50_000_000_000;
pub const API_CENTS: i32 = 1;
pub const MAX_STORED_INPUT_BYTES: i64 = 50_000_000_000;
/// Default for `CONVT_MAX_STORED_JOBS`: unexpired job prefixes an account may
/// hold. Every job, finished or not, holds one for 24 hours, so this is also the
/// account's conversions per rolling 24 hours.
pub const MAX_STORED_JOBS: i64 = 100;

pub fn max_stored_jobs() -> i64 {
    static LIMIT: std::sync::OnceLock<i64> = std::sync::OnceLock::new();
    *LIMIT.get_or_init(|| {
        std::env::var("CONVT_MAX_STORED_JOBS")
            .ok()
            .and_then(|v| v.parse::<i64>().ok())
            .filter(|n| *n > 0)
            .unwrap_or(MAX_STORED_JOBS)
    })
}

#[derive(Clone, Debug)]
pub struct Principal {
    pub user_id: String,
    pub key_id: Option<String>,
}
impl Principal {
    pub fn source(&self) -> &'static str {
        if self.key_id.is_some() { "api" } else { "web" }
    }
}
#[derive(Deserialize, Clone)]
#[serde(deny_unknown_fields)]
pub struct CreateJob {
    pub input_format: String,
    pub target_format: String,
    /// Exact upload size. Absent when `source_url` supplies the input.
    #[serde(default)]
    pub input_bytes: Option<i64>,
    /// Conversion settings. Omitted means the engines' defaults.
    #[serde(default)]
    pub options: convt_core::Options,
    /// An https URL the worker downloads instead of an upload. API keys only.
    #[serde(default)]
    pub source_url: Option<String>,
    /// An https URL that gets a signed POST when the job finishes.
    #[serde(default)]
    pub webhook_url: Option<String>,
    /// Hex SHA-256 of the file the client will upload. Not checked against the
    /// upload; it joins the Idempotency-Key fingerprint, so a retry that would
    /// upload different bytes under the same key is refused.
    #[serde(default)]
    pub input_sha256: Option<String>,
}
impl CreateJob {
    /// The parameters that identify a request for `Idempotency-Key` reuse checks.
    pub fn fingerprint(&self, route: &str, extra: &str) -> String {
        use sha2::{Digest, Sha256};
        let canonical = serde_json::json!([
            route,
            self.input_format,
            self.target_format,
            self.input_bytes,
            self.options,
            self.source_url,
            self.webhook_url,
            self.input_sha256,
            extra,
        ]);
        hex::encode(Sha256::digest(canonical.to_string().as_bytes()))
    }
}
/// A client-chosen key and the fingerprint of the request that carried it.
pub struct Idempotency {
    pub key: String,
    pub request_hash: String,
}
/// Server switches for optional inputs. Checked after an idempotent replay is
/// resolved, so a client can always recover a job it already created.
#[derive(Clone, Copy, Debug)]
pub struct Features {
    pub source_urls: bool,
    pub webhooks: bool,
}
impl Features {
    pub const ALL: Self = Self {
        source_urls: true,
        webhooks: true,
    };
}
pub struct Created {
    pub job: Job,
    /// The key matched an earlier request; no new reservation was made.
    pub replayed: bool,
}
#[derive(FromRow, Serialize, Clone, Debug)]
pub struct Job {
    pub id: String,
    pub user_id: String,
    pub source: String,
    pub api_key_id: Option<String>,
    pub subscription_id: Option<String>,
    pub quota_period_start: Option<DateTime<Utc>>,
    pub status: String,
    pub input_format: String,
    pub target_format: String,
    pub options: sqlx::types::Json<convt_core::Options>,
    pub input_key: Option<String>,
    pub input_bytes: Option<i64>,
    pub output_keys: sqlx::types::Json<Vec<String>>,
    pub attempt: i32,
    pub max_attempts: i32,
    pub lease_owner: Option<String>,
    pub lease_expires_at: Option<DateTime<Utc>>,
    pub reserved_bytes: i64,
    pub reserved_cents: i32,
    pub reservation: String,
    pub error_code: Option<String>,
    pub expires_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
    pub source_url: Option<String>,
    pub webhook_url: Option<String>,
}
#[derive(FromRow)]
struct Subscription {
    id: String,
    period: DateTime<Utc>,
    spend_cap_cents: Option<i32>,
}

pub async fn create(pool: &PgPool, who: &Principal, data: &CreateJob) -> Result<Job, ApiError> {
    Ok(create_with(pool, who, data, None, Features::ALL).await?.job)
}

pub async fn create_with(
    pool: &PgPool,
    who: &Principal,
    data: &CreateJob,
    idempotency: Option<&Idempotency>,
    features: Features,
) -> Result<Created, ApiError> {
    let policy = crate::net_guard::Policy::from_env();
    match (&data.source_url, data.input_bytes) {
        (Some(_), Some(_)) => {
            return Err(ApiError::bad(
                "invalid_request",
                "Send input_bytes for an upload or source_url for a download, not both.",
            ));
        }
        (Some(url), None) => {
            if who.key_id.is_none() {
                return Err(ApiError::bad(
                    "invalid_request",
                    "source_url needs an API key.",
                ));
            }
            crate::net_guard::check(url, policy)
                .map_err(|e| ApiError::bad("invalid_source_url", &format!("source_url {e}.")))?;
        }
        (None, None) => {
            return Err(ApiError::bad(
                "invalid_request",
                "input_bytes is required unless source_url is set.",
            ));
        }
        (None, Some(bytes)) if bytes <= 0 || bytes > MAX_FILE_BYTES => {
            return Err(ApiError::bad(
                "file_too_large",
                "Files must contain 1 byte to 2 GB.",
            ));
        }
        (None, Some(_)) => {}
    }
    if let Some(hash) = &data.input_sha256
        && !(hash.len() == 64 && hash.bytes().all(|b| b.is_ascii_hexdigit()))
    {
        return Err(ApiError::bad(
            "invalid_request",
            "input_sha256 must be 64 hexadecimal characters.",
        ));
    }
    if let Some(url) = &data.webhook_url {
        crate::net_guard::check(url, policy)
            .map_err(|e| ApiError::bad("invalid_webhook_url", &format!("webhook_url {e}.")))?;
    }
    let formats = convt_core::FORMATS;
    if !formats.iter().any(|f| f.id == data.input_format)
        || !formats.iter().any(|f| f.id == data.target_format)
        || data.input_format == data.target_format
    {
        return Err(ApiError::bad(
            "unsupported_format",
            "Choose distinct supported input and target formats.",
        ));
    }
    if let Err(e) = validate_options(&data.options, &data.target_format) {
        return Err(ApiError::bad("invalid_options", &e));
    }
    let capabilities: serde_json::Value =
        serde_json::from_str(include_str!("../cloud-formats.json"))
            .expect("generated capabilities");
    let supported = capabilities["formats"].as_array().unwrap().iter().any(|f| {
        f["id"] == data.input_format
            && f["targets"]
                .as_array()
                .unwrap()
                .iter()
                .any(|target| target == &data.target_format)
    });
    if !supported {
        return Err(ApiError::bad(
            "unsupported_format",
            "This input cannot reach the chosen target format.",
        ));
    }
    let mut tx = pool.begin().await?;
    // Both API and Pro share a storage budget. It is independent of conversion
    // charges and persists after cancellation until expiry cleanup succeeds.
    sqlx::query!(
        r#"select pg_advisory_xact_lock(hashtextextended($1, 9))::text as "locked!""#,
        &who.user_id
    )
    .fetch_one(&mut *tx)
    .await?;
    // Under the account lock, a key either names a live job or is free. A key
    // whose job expired is released so it can be used again.
    if let Some(idem) = idempotency {
        sqlx::query!(r#"update cloud_jobs set idempotency_key=null where user_id=$1 and idempotency_key=$2 and expires_at<=now()"#, &who.user_id, &idem.key).execute(&mut *tx).await?;
        let existing = sqlx::query!(
            r#"select id, request_hash from cloud_jobs where user_id=$1 and idempotency_key=$2"#,
            &who.user_id,
            &idem.key
        )
        .fetch_optional(&mut *tx)
        .await?;
        if let Some(existing) = existing {
            tx.commit().await?;
            if existing.request_hash.as_deref() != Some(idem.request_hash.as_str()) {
                return Err(ApiError::conflict(
                    "idempotency_key_reused",
                    "This Idempotency-Key was used for a different request in the last 24 hours. Use a new key.",
                ));
            }
            return Ok(Created {
                job: owned(pool, who, &existing.id).await?,
                replayed: true,
            });
        }
    }
    if data.source_url.is_some() && !features.source_urls {
        return Err(ApiError::bad(
            "source_url_unavailable",
            "source_url is not enabled on this server yet. Upload the file instead.",
        ));
    }
    if data.webhook_url.is_some() && !features.webhooks {
        return Err(ApiError::bad(
            "webhooks_unavailable",
            "Webhooks are not enabled on this server yet. Poll the job instead.",
        ));
    }
    let stored = sqlx::query!(r#"select count(*)::bigint as "count!", coalesce(sum(input_bytes),0)::bigint as "bytes!" from cloud_jobs where user_id=$1 and input_key is not null"#, &who.user_id).fetch_one(&mut *tx).await?;
    if stored.count >= max_stored_jobs()
        || stored.bytes + data.input_bytes.unwrap_or(0) > MAX_STORED_INPUT_BYTES
    {
        return Err(ApiError::forbidden(
            "storage_limit_reached",
            "Outstanding uploads have reached the storage limit. Try again after their 24-hour cleanup.",
        ));
    }
    // P7 cap edits lock this same row. Pro allowance is a UTC calendar month,
    // including annual subscriptions. API billing follows its provider period.
    let kind = if who.key_id.is_some() { "api" } else { "pro" };
    let sub = sqlx::query_as!(Subscription, r#"select id, case when kind='pro' then (date_trunc('month',now() at time zone 'UTC') at time zone 'UTC') else coalesce(current_period_start,(date_trunc('month',now() at time zone 'UTC') at time zone 'UTC')) end as "period!", spend_cap_cents from subscriptions where user_id=$1 and kind=$2 and status='active' and (ended_at is null or ended_at>now()) and (current_period_start is null or current_period_start<=now()) and (current_period_end>now() or (kind='api' and current_period_end is null)) and (kind='pro' or card_seen_at is not null) order by created_at desc limit 1 for update"#, &who.user_id, kind).fetch_optional(&mut *tx).await?.ok_or_else(|| ApiError::forbidden("not_enrolled", "An active paid subscription is required."))?;
    let settled: i64 = sqlx::query_scalar!(r#"select coalesce(sum(case when $3='api' then e.amount_cents else e.quantity end),0)::bigint as "value!" from usage_events e left join usage_events original on original.id=e.corrects where (e.subscription_id=$1 or ($3='pro' and e.subscription_id in (select s.id from subscriptions s where s.user_id=$4 and s.kind='pro'))) and e.occurred_at >= $2 and (e.kind=case when $3='api' then 'api_conversion' else 'pro_bytes' end or (e.kind='correction' and original.kind=case when $3='api' then 'api_conversion' else 'pro_bytes' end))"#, &sub.id, sub.period, kind, &who.user_id).fetch_one(&mut *tx).await?;
    // Open commitments survive cap edits and period rollover. Pro is one allowance
    // per account: usage and reservations count across all of its Pro rows
    // (a subscription and prepaid Desktop + Cloud years), under the user lock above.
    let reserved: i64 = sqlx::query_scalar!(r#"select coalesce(sum(case when $2='api' then reserved_cents else reserved_bytes end),0)::bigint as "value!" from cloud_jobs where (subscription_id=$1 or ($2='pro' and subscription_id in (select s.id from subscriptions s where s.user_id=$3 and s.kind='pro'))) and reservation='open'"#, &sub.id, kind, &who.user_id).fetch_one(&mut *tx).await?;
    let input_bytes = data.input_bytes.unwrap_or(0);
    let charge = if kind == "api" {
        i64::from(API_CENTS)
    } else {
        input_bytes
    };
    let cap = if kind == "api" {
        i64::from(sub.spend_cap_cents.unwrap_or(0))
    } else {
        PRO_MONTH_BYTES
    };
    if settled + reserved + charge > cap {
        return Err(ApiError::forbidden(
            "limit_reached",
            "Your allowance or spend cap has been reached.",
        ));
    }
    let id = new_id("job");
    // A source_url job is queued at once: the worker downloads its input. Its
    // input_key names the prefix expiry cleanup deletes; nothing is uploaded there.
    let (status, input_key) = if data.source_url.is_some() {
        ("queued", format!("{id}/source"))
    } else {
        ("created", format!("{id}/upload"))
    };
    let job = sqlx::query_as!(Job, r#"insert into cloud_jobs (id,user_id,source,api_key_id,subscription_id,quota_period_start,input_format,target_format,options,input_key,input_bytes,reserved_bytes,reserved_cents,expires_at,status,queued_at,source_url,webhook_url,idempotency_key,request_hash) values ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,now()+interval '24 hours',$14,case when $14='queued' then now() end,$15,$16,$17,$18) returning id,user_id,source,api_key_id,subscription_id,quota_period_start,status,input_format,target_format,options as "options: _",input_key,input_bytes,output_keys as "output_keys: _",attempt,max_attempts,lease_owner,lease_expires_at,reserved_bytes,reserved_cents,reservation,error_code,expires_at,created_at,source_url,webhook_url"#, &id, &who.user_id, who.source(), who.key_id.as_deref(), &sub.id, sub.period, &data.input_format, &data.target_format, sqlx::types::Json(&data.options) as _, input_key, data.input_bytes, if kind=="pro" {input_bytes} else {0}, if kind=="api" {API_CENTS} else {0}, status, data.source_url.as_deref(), data.webhook_url.as_deref(), idempotency.map(|i| i.key.as_str()), idempotency.map(|i| i.request_hash.as_str())).fetch_one(&mut *tx).await?;
    tx.commit().await?;
    Ok(Created {
        job,
        replayed: false,
    })
}
/// The same checks the CLI makes before converting, so a bad option fails
/// at reservation instead of in the worker.
pub fn validate_options(options: &convt_core::Options, target: &str) -> Result<(), String> {
    options.validate().map_err(|e| e.to_string())?;
    if let Some(to) = convt_core::format_by_id(target)
        && options.background == Some(convt_core::Background::Transparent)
        && to.category == convt_core::Category::Image
        && !to.keeps_transparency()
    {
        return Err(format!(
            "{} can't store transparency; choose a background color such as white or black",
            to.name
        ));
    }
    Ok(())
}
/// A live job of the caller's account. A web credential (the browser converter's
/// five-minute token) sees only web and desktop jobs, never API-key jobs.
pub async fn owned(pool: &PgPool, who: &Principal, id: &str) -> Result<Job, ApiError> {
    sqlx::query_as!(Job, r#"select id,user_id,source,api_key_id,subscription_id,quota_period_start,status,input_format,target_format,options as "options: _",input_key,input_bytes,output_keys as "output_keys: _",attempt,max_attempts,lease_owner,lease_expires_at,reserved_bytes,reserved_cents,reservation,error_code,expires_at,created_at,source_url,webhook_url from cloud_jobs where id=$1 and user_id=$2 and expires_at>now() and ($3 or source<>'api')"#, id, &who.user_id, who.key_id.is_some())
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| ApiError::not_found("Job not found."))
}
/// Any job by id, for background delivery. Never use it to answer a request.
pub async fn by_id(pool: &PgPool, id: &str) -> sqlx::Result<Option<Job>> {
    sqlx::query_as!(Job, r#"select id,user_id,source,api_key_id,subscription_id,quota_period_start,status,input_format,target_format,options as "options: _",input_key,input_bytes,output_keys as "output_keys: _",attempt,max_attempts,lease_owner,lease_expires_at,reserved_bytes,reserved_cents,reservation,error_code,expires_at,created_at,source_url,webhook_url from cloud_jobs where id=$1"#, id)
        .fetch_optional(pool)
        .await
}
/// Hands a running attempt back to the queue when its worker shuts down, so a
/// deploy does not fail the job. `max_attempts` grows with `attempt`, so the
/// handoff costs no retry and the fencing token keeps increasing.
pub async fn requeue(pool: &PgPool, job: &Job) -> sqlx::Result<bool> {
    Ok(sqlx::query!(r#"update cloud_jobs set status='queued',max_attempts=max_attempts+1,lease_owner=null,lease_expires_at=null,queued_at=now(),updated_at=now() where id=$1 and attempt=$2 and lease_owner=$3 and status='running' and expires_at>now() and reservation='open'"#, &job.id, job.attempt, job.lease_owner.as_deref()).execute(pool).await?.rows_affected()==1)
}
pub async fn cancel(pool: &PgPool, who: &Principal, id: &str) -> Result<Job, ApiError> {
    let _ = owned(pool, who, id).await?;
    sqlx::query!(r#"update cloud_jobs set status='cancelled',reservation='released',lease_expires_at=null,finished_at=now(),updated_at=now() where id=$1 and user_id=$2 and status not in ('succeeded','failed','cancelled')"#, id, &who.user_id).execute(pool).await?;
    owned(pool, who, id).await
}
pub async fn claim(pool: &PgPool, worker: &str) -> sqlx::Result<Option<Job>> {
    sqlx::query!(r#"update cloud_jobs set status='failed',reservation='released',error_code='expired_or_exhausted',lease_expires_at=null,finished_at=now() where reservation='open' and (expires_at<=now() or (status='running' and lease_expires_at<=now() and attempt>=max_attempts))"#).execute(pool).await?;
    sqlx::query_as!(Job, r#"with candidate as (select id from cloud_jobs where expires_at>now() and attempt<max_attempts and (status='queued' or (status='running' and lease_expires_at<=now())) order by queued_at for update skip locked limit 1) update cloud_jobs j set status='running',attempt=attempt+1,lease_owner=$1,lease_expires_at=now()+interval '30 seconds',started_at=now(),updated_at=now() from candidate c where j.id=c.id returning j.id,j.user_id,j.source,j.api_key_id,j.subscription_id,j.quota_period_start,j.status,j.input_format,j.target_format,j.options as "options: _",j.input_key,j.input_bytes,j.output_keys as "output_keys: _",j.attempt,j.max_attempts,j.lease_owner,j.lease_expires_at,j.reserved_bytes,j.reserved_cents,j.reservation,j.error_code,j.expires_at,j.created_at,j.source_url,j.webhook_url"#, worker).fetch_optional(pool).await
}
pub async fn renew(pool: &PgPool, job: &Job) -> sqlx::Result<bool> {
    Ok(sqlx::query!(r#"update cloud_jobs set lease_expires_at=now()+interval '30 seconds',updated_at=now() where id=$1 and attempt=$2 and lease_owner=$3 and status='running' and lease_expires_at>now() and expires_at>now()"#, &job.id, job.attempt, job.lease_owner.as_deref()).execute(pool).await?.rows_affected()==1)
}
pub async fn finish(
    pool: &PgPool,
    job: &Job,
    outputs: &[String],
    error: Option<&str>,
) -> sqlx::Result<bool> {
    let mut tx = pool.begin().await?;
    // Serialize completion with new reservations and P7 cap changes. Pro is one
    // allowance across every Pro row, so a job settling on an older row must take
    // the account lock `create` takes (in the same order: account, then row), or
    // its bytes can fall between `create`'s settled and reserved reads.
    sqlx::query!(
        r#"select pg_advisory_xact_lock(hashtextextended($1, 9))::text as "locked!""#,
        &job.user_id
    )
    .fetch_one(&mut *tx)
    .await?;
    sqlx::query!(
        r#"select id from subscriptions where id=$1 for update"#,
        job.subscription_id.as_deref()
    )
    .fetch_optional(&mut *tx)
    .await?;
    let changed = sqlx::query!(r#"update cloud_jobs set status=$4, output_keys=$5, reservation=$6, error_code=$7, lease_expires_at=null, finished_at=now(),updated_at=now() where id=$1 and attempt=$2 and lease_owner=$3 and status='running' and lease_expires_at>now() and expires_at>now() and reservation='open'"#, &job.id, job.attempt, job.lease_owner.as_deref(), if error.is_some(){"failed"}else{"succeeded"}, serde_json::json!(outputs), if error.is_some(){"released"}else{"settled"}, error).execute(&mut *tx).await?.rows_affected()==1;
    if changed && error.is_none() {
        // Charge the reservation's period, even if execution crossed rollover.
        sqlx::query!(r#"insert into usage_events (id,user_id,subscription_id,api_key_id,job_id,kind,quantity,amount_cents,occurred_at) values ($1,$2,$3,$4,$5,$6,$7,$8,$9) on conflict (job_id,kind) where corrects is null do nothing"#, new_id("use"), &job.user_id, job.subscription_id.as_deref(), job.api_key_id.as_deref(), &job.id, if job.source=="api" {"api_conversion"}else{"pro_bytes"}, if job.source=="api" {1}else{job.reserved_bytes}, job.reserved_cents, job.created_at).execute(&mut *tx).await?;
    }
    tx.commit().await?;
    Ok(changed)
}

/// The host reaper must retain only containers belonging to a live attempt.
pub async fn attempt_is_live(pool: &PgPool, id: &str, attempt: i32) -> sqlx::Result<bool> {
    sqlx::query_scalar!(r#"select exists(select 1 from cloud_jobs where id=$1 and attempt=$2 and status='running' and lease_expires_at>now() and expires_at>now()) as "live!""#, id, attempt).fetch_one(pool).await
}
