//! Deletes expired sign-in codes and rate-limit rows every 10 minutes. The web app
//! turns off Better Auth's own expiry deletes (`verification.disableCleanup`), so
//! this is the only bulk delete of verification rows.

use std::time::Duration;

use chrono::{DateTime, Utc};
use sqlx::PgPool;

pub const INTERVAL: Duration = Duration::from_secs(600);

/// Rate-limit windows are at most an hour; keep a day of rows to be safe.
const RATE_LIMIT_KEEP_MS: i64 = 86_400_000;

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Removed {
    pub verifications: u64,
    pub rate_limits: u64,
    pub otp_send_limits: u64,
}

pub async fn run_once(pool: &PgPool, now: DateTime<Utc>) -> sqlx::Result<Removed> {
    // Markers first, then the rest, as separate statements: issuing a code locks
    // its `otp-issued:` marker and then the code row, so neither statement holds
    // one kind of row while waiting for the other, and no lock cycle can form.
    let markers = sqlx::query!(
        "delete from verifications where identifier like 'otp-issued:%' and expires_at < $1",
        now
    )
    .execute(pool)
    .await?
    .rows_affected();
    let others = sqlx::query!(
        "delete from verifications where identifier not like 'otp-issued:%' and expires_at < $1",
        now
    )
    .execute(pool)
    .await?
    .rows_affected();
    let verifications = markers + others;
    let cutoff = now.timestamp_millis() - RATE_LIMIT_KEEP_MS;
    let rate_limits = sqlx::query!("delete from rate_limits where last_request < $1", cutoff)
        .execute(pool)
        .await?
        .rows_affected();
    let otp_send_limits = sqlx::query!("delete from otp_send_limits where expires_at < $1", now)
        .execute(pool)
        .await?
        .rows_affected();
    Ok(Removed {
        verifications,
        rate_limits,
        otp_send_limits,
    })
}

pub fn spawn(pool: PgPool) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(INTERVAL);
        loop {
            tick.tick().await;
            match run_once(&pool, Utc::now()).await {
                Ok(removed) => tracing::debug!(?removed, "cleanup"),
                Err(e) => tracing::warn!(error = %e, "cleanup failed"),
            }
        }
    })
}
