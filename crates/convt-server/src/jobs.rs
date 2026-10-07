//! Transactional reservations and attempt-fenced Postgres queue.
use crate::{ids::new_id, routes::ApiError};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgPool};

pub const MAX_FILE_BYTES: i64 = 2_000_000_000;
// User-facing Cloud is unlimited. This 50 GB input cap stays as a hidden safety
// limit. Soft compute-cost budget (~$8 / period) is CNV-30; Pro jobs record
// amount_cents = 0 today.
pub const PRO_MONTH_BYTES: i64 = 50_000_000_000;
pub const API_CENTS: i32 = 1;
pub const MAX_STORED_INPUT_BYTES: i64 = 50_000_000_000;
pub const MAX_STORED_JOBS: i64 = 100;

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
    pub input_bytes: i64,
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
}
#[derive(FromRow)]
struct Subscription {
    id: String,
    period: DateTime<Utc>,
    spend_cap_cents: Option<i32>,
}

pub async fn create(pool: &PgPool, who: &Principal, data: &CreateJob) -> Result<Job, ApiError> {
    if data.input_bytes <= 0 || data.input_bytes > MAX_FILE_BYTES {
        return Err(ApiError::bad(
            "file_too_large",
            "Files must contain 1 byte to 2 GB.",
        ));
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
    let stored = sqlx::query!(r#"select count(*)::bigint as "count!", coalesce(sum(input_bytes),0)::bigint as "bytes!" from cloud_jobs where user_id=$1 and input_key is not null"#, &who.user_id).fetch_one(&mut *tx).await?;
    if stored.count >= MAX_STORED_JOBS || stored.bytes + data.input_bytes > MAX_STORED_INPUT_BYTES {
        return Err(ApiError::forbidden(
            "storage_limit_reached",
            "Outstanding uploads have reached the storage limit. Try again after their 24-hour cleanup.",
        ));
    }
    // P7 cap edits lock this same row. Pro allowance is a UTC calendar month,
    // including annual subscriptions. API billing follows its provider period.
    let kind = if who.key_id.is_some() { "api" } else { "pro" };
    let sub = sqlx::query_as!(Subscription, r#"select id, case when kind='pro' then (date_trunc('month',now() at time zone 'UTC') at time zone 'UTC') else coalesce(current_period_start,(date_trunc('month',now() at time zone 'UTC') at time zone 'UTC')) end as "period!", spend_cap_cents from subscriptions where user_id=$1 and kind=$2 and (status='active' or (kind='pro' and status='trialing')) and (ended_at is null or ended_at>now()) and (current_period_start is null or current_period_start<=now()) and (current_period_end>now() or (kind='api' and current_period_end is null)) and (kind='pro' or card_seen_at is not null) order by created_at desc limit 1 for update"#, &who.user_id, kind).fetch_optional(&mut *tx).await?.ok_or_else(|| ApiError::forbidden("not_enrolled", if kind == "pro" { "An active Pro subscription or trial is required." } else { "An active paid subscription is required." }))?;
    let settled: i64 = sqlx::query_scalar!(r#"select coalesce(sum(case when $3='api' then e.amount_cents else e.quantity end),0)::bigint as "value!" from usage_events e left join usage_events original on original.id=e.corrects where e.subscription_id=$1 and e.occurred_at >= $2 and (e.kind=case when $3='api' then 'api_conversion' else 'pro_bytes' end or (e.kind='correction' and original.kind=case when $3='api' then 'api_conversion' else 'pro_bytes' end))"#, &sub.id, sub.period, kind).fetch_one(&mut *tx).await?;
    // Open commitments survive cap edits and period rollover.
    let reserved: i64 = sqlx::query_scalar!(r#"select coalesce(sum(case when $2='api' then reserved_cents else reserved_bytes end),0)::bigint as "value!" from cloud_jobs where subscription_id=$1 and reservation='open'"#, &sub.id, kind).fetch_one(&mut *tx).await?;
    let charge = if kind == "api" {
        i64::from(API_CENTS)
    } else {
        data.input_bytes
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
    let job = sqlx::query_as!(Job, r#"insert into cloud_jobs (id,user_id,source,api_key_id,subscription_id,quota_period_start,input_format,target_format,input_key,input_bytes,reserved_bytes,reserved_cents,expires_at) values ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,now()+interval '24 hours') returning id,user_id,source,api_key_id,subscription_id,quota_period_start,status,input_format,target_format,input_key,input_bytes,output_keys as "output_keys: _",attempt,max_attempts,lease_owner,lease_expires_at,reserved_bytes,reserved_cents,reservation,error_code,expires_at,created_at"#, &id, &who.user_id, who.source(), who.key_id.as_deref(), &sub.id, sub.period, &data.input_format, &data.target_format, format!("{id}/upload"), data.input_bytes, if kind=="pro" {data.input_bytes} else {0}, if kind=="api" {API_CENTS} else {0}).fetch_one(&mut *tx).await?;
    tx.commit().await?;
    Ok(job)
}
pub async fn owned(pool: &PgPool, who: &Principal, id: &str) -> Result<Job, ApiError> {
    sqlx::query_as!(Job, r#"select id,user_id,source,api_key_id,subscription_id,quota_period_start,status,input_format,target_format,input_key,input_bytes,output_keys as "output_keys: _",attempt,max_attempts,lease_owner,lease_expires_at,reserved_bytes,reserved_cents,reservation,error_code,expires_at,created_at from cloud_jobs where id=$1 and user_id=$2 and expires_at>now()"#, id, &who.user_id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| ApiError::not_found("Job not found."))
}
pub async fn cancel(pool: &PgPool, who: &Principal, id: &str) -> Result<Job, ApiError> {
    let _ = owned(pool, who, id).await?;
    sqlx::query!(r#"update cloud_jobs set status='cancelled',reservation='released',lease_expires_at=null,finished_at=now(),updated_at=now() where id=$1 and user_id=$2 and status not in ('succeeded','failed','cancelled')"#, id, &who.user_id).execute(pool).await?;
    owned(pool, who, id).await
}
pub async fn claim(pool: &PgPool, worker: &str) -> sqlx::Result<Option<Job>> {
    sqlx::query!(r#"update cloud_jobs set status='failed',reservation='released',error_code='expired_or_exhausted',lease_expires_at=null,finished_at=now() where reservation='open' and (expires_at<=now() or (status='running' and lease_expires_at<=now() and attempt>=max_attempts))"#).execute(pool).await?;
    sqlx::query_as!(Job, r#"with candidate as (select id from cloud_jobs where expires_at>now() and attempt<max_attempts and (status='queued' or (status='running' and lease_expires_at<=now())) order by queued_at for update skip locked limit 1) update cloud_jobs j set status='running',attempt=attempt+1,lease_owner=$1,lease_expires_at=now()+interval '30 seconds',started_at=now(),updated_at=now() from candidate c where j.id=c.id returning j.id,j.user_id,j.source,j.api_key_id,j.subscription_id,j.quota_period_start,j.status,j.input_format,j.target_format,j.input_key,j.input_bytes,j.output_keys as "output_keys: _",j.attempt,j.max_attempts,j.lease_owner,j.lease_expires_at,j.reserved_bytes,j.reserved_cents,j.reservation,j.error_code,j.expires_at,j.created_at"#, worker).fetch_optional(pool).await
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
    // Serialize completion with new reservations and P7 cap changes.
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
