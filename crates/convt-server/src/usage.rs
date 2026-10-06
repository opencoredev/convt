//! Usage facts. One row per job and kind, so recording the same job twice (a retry,
//! a crash after commit) bills once. Rows are append-only in the database.

use chrono::{DateTime, Utc};
use sqlx::PgPool;

use crate::ids::new_id;

#[derive(Debug, Clone)]
pub struct NewUsage<'a> {
    pub user_id: &'a str,
    pub subscription_id: Option<&'a str>,
    pub api_key_id: Option<&'a str>,
    pub job_id: &'a str,
    pub kind: &'a str,
    pub quantity: i64,
    pub amount_cents: i32,
    pub occurred_at: DateTime<Utc>,
}

/// Inserts the usage row for a job, or does nothing if that job's row exists.
/// Returns whether a row was inserted.
pub async fn record(pool: &PgPool, usage: &NewUsage<'_>) -> sqlx::Result<bool> {
    let id = new_id("use");
    let inserted = sqlx::query_scalar!(
        "insert into usage_events
           (id, user_id, subscription_id, api_key_id, job_id, kind, quantity, amount_cents, occurred_at)
         values ($1, $2, $3, $4, $5, $6, $7, $8, $9)
         on conflict (job_id, kind) where corrects is null do nothing
         returning id",
        id,
        usage.user_id,
        usage.subscription_id,
        usage.api_key_id,
        usage.job_id,
        usage.kind,
        usage.quantity,
        usage.amount_cents,
        usage.occurred_at,
    )
    .fetch_optional(pool)
    .await?;
    Ok(inserted.is_some())
}
