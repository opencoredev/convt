//! Job webhooks. A job created with `webhook_url` gets one signed `POST` when it
//! reaches `succeeded`, `failed` or `cancelled`. Delivery state lives on the job
//! row, so any API replica can deliver and a restart loses nothing. Delivery is at
//! least once; receivers dedupe on the `Convt-Event-Id` header.
//!
//! Each account's signing secret is derived from `CONVT_WEBHOOK_SECRET`, the
//! account id and a version in `webhook_secrets`, so the database holds no secret.
//! `GET /v1/webhooks/secret` returns it to an API key (never to the browser
//! converter's short-lived web credential); `POST /v1/webhooks/secret/rotate`
//! bumps the version. Rotating the server secret rotates every account's.

use std::time::Duration;

use hmac::{Hmac, Mac};
use sha2::Sha256;
use sqlx::PgPool;

use crate::net_guard::{self, Policy};
use crate::routes::JobView;
use crate::storage::Storage;

/// Sends before a delivery is abandoned. The waits between them add up to
/// 2 h 42 min 40 s.
pub const MAX_ATTEMPTS: i32 = 8;
const BACKOFF_SECONDS: [i64; 7] = [10, 30, 120, 600, 1800, 3600, 3600];

/// Rows claimed per pass. Each send is bounded (10 s DNS plus 10 s HTTP) and they
/// run concurrently, so a pass ends well inside the claim's lease.
const BATCH: i64 = 10;

pub fn account_secret(master: &str, user_id: &str, version: i32) -> String {
    let mut mac = Hmac::<Sha256>::new_from_slice(master.as_bytes()).expect("any key length");
    mac.update(b"convt-webhook-v1:");
    mac.update(user_id.as_bytes());
    mac.update(b":");
    mac.update(version.to_string().as_bytes());
    format!("whsec_{}", hex::encode(mac.finalize().into_bytes()))
}

/// The account's current secret version (1 until it first rotates).
pub async fn secret_version(pool: &PgPool, user_id: &str) -> sqlx::Result<i32> {
    Ok(sqlx::query_scalar!(
        r#"select version from webhook_secrets where user_id=$1"#,
        user_id
    )
    .fetch_optional(pool)
    .await?
    .unwrap_or(1))
}

/// Moves the account to a new secret and returns its version.
pub async fn rotate(pool: &PgPool, user_id: &str) -> sqlx::Result<i32> {
    sqlx::query_scalar!(
        r#"insert into webhook_secrets (user_id, version) values ($1, 2) on conflict (user_id) do update set version=webhook_secrets.version+1, updated_at=now() returning version"#,
        user_id
    )
    .fetch_one(pool)
    .await
}

/// `t=<unix seconds>,v1=<hex HMAC-SHA256 of "<t>.<body>">`, Stripe's scheme.
pub fn signature(secret: &str, timestamp: i64, body: &[u8]) -> String {
    let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).expect("any key length");
    mac.update(timestamp.to_string().as_bytes());
    mac.update(b".");
    mac.update(body);
    format!(
        "t={timestamp},v1={}",
        hex::encode(mac.finalize().into_bytes())
    )
}

#[derive(sqlx::FromRow)]
struct Due {
    id: String,
    user_id: String,
    webhook_url: String,
    webhook_attempts: i32,
    /// The lease this pass set; every later update must still find it.
    lease: chrono::DateTime<chrono::Utc>,
    secret_version: i32,
}

/// Claims due deliveries and sends them. Returns how many were delivered.
pub async fn deliver_due(
    pool: &PgPool,
    storage: &dyn Storage,
    master: &str,
    policy: Policy,
) -> anyhow::Result<usize> {
    // The claim moves `webhook_next_at` two minutes ahead: a lease that keeps
    // other replicas off the row, and the fencing token for the updates below.
    // `webhook_attempts` counts sends that failed, so a process that dies after
    // claiming loses no attempt: the lease lapses and the row is claimed again.
    let due = sqlx::query_as!(Due, r#"update cloud_jobs j set webhook_next_at=now()+interval '120 seconds' from (select id from cloud_jobs where webhook_url is not null and webhook_delivered_at is null and webhook_attempts<$1 and status in ('succeeded','failed','cancelled') and (webhook_next_at is null or webhook_next_at<=now()) order by coalesce(webhook_next_at, updated_at) limit $2 for update skip locked) c where j.id=c.id returning j.id, j.user_id, j.webhook_url as "webhook_url!", j.webhook_attempts, j.webhook_next_at as "lease!", coalesce((select version from webhook_secrets w where w.user_id=j.user_id),1) as "secret_version!""#, MAX_ATTEMPTS, BATCH).fetch_all(pool).await?;
    use futures_util::StreamExt as _;
    // Each result is recorded as soon as its send finishes, so a fast receiver's
    // acknowledgment is saved even if a slow one in the same pass outlives a
    // shutdown.
    let mut sends = futures_util::stream::FuturesUnordered::new();
    for row in &due {
        sends.push(async move {
            // Building the event and sending it stay well inside the lease.
            let result = tokio::time::timeout(
                Duration::from_secs(22),
                send(pool, storage, master, policy, row),
            )
            .await
            .unwrap_or_else(|_| Err(anyhow::anyhow!("delivery timed out")));
            (row, result)
        });
    }
    let mut delivered = 0;
    while let Some((row, result)) = sends.next().await {
        match result {
            Ok(()) => {
                delivered += 1;
                sqlx::query!(
                    r#"update cloud_jobs set webhook_delivered_at=now(), webhook_next_at=null where id=$1 and webhook_next_at=$2"#,
                    &row.id,
                    row.lease
                )
                .execute(pool)
                .await?;
            }
            Err(error) => {
                // Never log the URL: it may carry a receiver's token.
                tracing::info!(job_id = row.id, attempt = row.webhook_attempts + 1, error = %error, "webhook delivery failed");
                let wait =
                    BACKOFF_SECONDS[(row.webhook_attempts as usize).min(BACKOFF_SECONDS.len() - 1)];
                sqlx::query!(
                    r#"update cloud_jobs set webhook_attempts=webhook_attempts+1, webhook_next_at=now()+make_interval(secs => $3) where id=$1 and webhook_next_at=$2 and webhook_delivered_at is null"#,
                    &row.id,
                    row.lease,
                    wait as f64
                )
                .execute(pool)
                .await?;
            }
        }
    }
    Ok(delivered)
}

/// The event body: the job as `GET /v1/jobs/{id}` shows it, plus fresh output
/// URLs for a succeeded job that has not expired.
pub async fn event_body(
    pool: &PgPool,
    storage: &dyn Storage,
    id: &str,
) -> anyhow::Result<Option<serde_json::Value>> {
    let Some(job) = crate::jobs::by_id(pool, id).await? else {
        return Ok(None);
    };
    let kind = format!("job.{}", job.status);
    let mut outputs = Vec::new();
    let mut expires_in = None;
    let ttl = (job.expires_at - chrono::Utc::now()).num_seconds();
    if job.status == "succeeded" && ttl > 0 {
        let ttl = ttl.clamp(1, 300) as u32;
        for key in &job.output_keys.0 {
            outputs.push(serde_json::json!({"name": key.rsplit('/').next().unwrap_or("output"), "url": storage.download_url(key, ttl).await?}));
        }
        expires_in = Some(ttl);
    }
    let id = job.id.clone();
    let mut view = JobView::from(job);
    // A presigned source_url is a live credential; receivers don't need it.
    view.source_url = None;
    Ok(Some(serde_json::json!({
        "id": format!("evt_{}", id.trim_start_matches("job_")),
        "type": kind,
        "created_at": chrono::Utc::now(),
        "data": {"job": view, "outputs": outputs, "expires_in": expires_in},
    })))
}

async fn send(
    pool: &PgPool,
    storage: &dyn Storage,
    master: &str,
    policy: Policy,
    row: &Due,
) -> anyhow::Result<()> {
    let Some(event) = event_body(pool, storage, &row.id).await? else {
        anyhow::bail!("job disappeared");
    };
    let body = serde_json::to_vec(&event)?;
    let url = net_guard::check(&row.webhook_url, policy)?;
    let client = net_guard::client_for(&url, policy, Duration::from_secs(10)).await?;
    let now = chrono::Utc::now().timestamp();
    let secret = account_secret(master, &row.user_id, row.secret_version);
    let response = client
        .post(url)
        .header("content-type", "application/json")
        .header("convt-event-id", event["id"].as_str().unwrap_or_default())
        .header(
            "convt-event-type",
            event["type"].as_str().unwrap_or_default(),
        )
        .header(
            "convt-delivery-attempt",
            (row.webhook_attempts + 1).to_string(),
        )
        .header("convt-signature", signature(&secret, now, &body))
        .body(body)
        .send()
        .await
        .map_err(|e| anyhow::anyhow!("{}", e.without_url()))?;
    anyhow::ensure!(
        response.status().is_success(),
        "receiver answered HTTP {}",
        response.status()
    );
    Ok(())
}

/// Delivers webhooks every two seconds. Stops claiming once the server starts
/// draining, so a deploy never leaves a claimed row waiting out its lease.
pub fn spawn(
    pool: PgPool,
    storage: std::sync::Arc<dyn Storage>,
    master: String,
    policy: Policy,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(Duration::from_secs(2));
        loop {
            tick.tick().await;
            if crate::routes::draining() {
                break;
            }
            if let Err(error) = deliver_due(&pool, storage.as_ref(), &master, policy).await {
                tracing::warn!(%error, "webhook dispatcher will retry");
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signature_matches_reference_vector() {
        // Same vectors as packages/sdk/test/webhooks.test.ts, computed with Python's hmac.
        let master = "master-secret-for-tests-only-0000";
        assert_eq!(
            account_secret(master, "usr_a", 1),
            "whsec_dacde83eb7c9531aa139eecf38be9e532cbfb6dec953176e73788d13e24fedb3"
        );
        assert_ne!(
            account_secret(master, "usr_a", 1),
            account_secret(master, "usr_b", 1)
        );
        assert_ne!(
            account_secret(master, "usr_a", 1),
            account_secret(master, "usr_a", 2)
        );
        assert_eq!(
            signature("whsec_test", 1_700_000_000, br#"{"ok":true}"#),
            "t=1700000000,v1=85876387ad9d6be57a04653bc0729da757049f58afb10ba6cac3bedaecf4fda3"
        );
    }
}
