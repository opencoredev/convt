//! Job webhooks. A job created with `webhook_url` gets one signed `POST` when it
//! reaches `succeeded`, `failed` or `cancelled`. Delivery state lives on the job
//! row, so any API replica can deliver and a restart loses nothing. Delivery is at
//! least once; receivers dedupe on the `Convt-Event-Id` header.
//!
//! Each account's signing secret is derived from `CONVT_WEBHOOK_SECRET` and the
//! account id, so the database holds no webhook secret. `GET /v1/webhooks/secret`
//! returns it. Rotating the server secret rotates every account's.

use std::time::Duration;

use hmac::{Hmac, Mac};
use sha2::Sha256;
use sqlx::PgPool;

use crate::net_guard::{self, Policy};
use crate::routes::JobView;
use crate::storage::Storage;

/// Attempts before a delivery is abandoned, spread over about five hours.
pub const MAX_ATTEMPTS: i32 = 8;
const BACKOFF_SECONDS: [i64; 8] = [10, 30, 120, 600, 1800, 3600, 3600, 3600];

pub fn account_secret(master: &str, user_id: &str) -> String {
    let mut mac = Hmac::<Sha256>::new_from_slice(master.as_bytes()).expect("any key length");
    mac.update(b"convt-webhook-v1:");
    mac.update(user_id.as_bytes());
    format!("whsec_{}", hex::encode(mac.finalize().into_bytes()))
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
}

/// Claims due deliveries and sends them. Returns how many were delivered.
pub async fn deliver_due(
    pool: &PgPool,
    storage: &dyn Storage,
    master: &str,
    policy: Policy,
) -> anyhow::Result<usize> {
    // The claim moves `webhook_next_at` a minute ahead, a lease that keeps other
    // replicas off the row while this one sends.
    let due = sqlx::query_as!(Due, r#"update cloud_jobs j set webhook_attempts=j.webhook_attempts+1, webhook_next_at=now()+interval '60 seconds' from (select id from cloud_jobs where webhook_url is not null and webhook_delivered_at is null and webhook_attempts<$1 and status in ('succeeded','failed','cancelled') and (webhook_next_at is null or webhook_next_at<=now()) order by webhook_next_at nulls first limit 20 for update skip locked) c where j.id=c.id returning j.id, j.user_id, j.webhook_url as "webhook_url!", j.webhook_attempts"#, MAX_ATTEMPTS).fetch_all(pool).await?;
    let mut delivered = 0;
    for row in due {
        let ok = match send(pool, storage, master, policy, &row).await {
            Ok(()) => true,
            Err(error) => {
                tracing::info!(job_id = row.id, attempt = row.webhook_attempts, %error, "webhook delivery failed");
                false
            }
        };
        if ok {
            delivered += 1;
            sqlx::query!(
                r#"update cloud_jobs set webhook_delivered_at=now(), webhook_next_at=null where id=$1"#,
                &row.id
            )
            .execute(pool)
            .await?;
        } else {
            let wait = BACKOFF_SECONDS[(row.webhook_attempts.max(1) as usize - 1).min(7)];
            sqlx::query!(
                r#"update cloud_jobs set webhook_next_at=now()+make_interval(secs => $2) where id=$1"#,
                &row.id,
                wait as f64
            )
            .execute(pool)
            .await?;
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
    Ok(Some(serde_json::json!({
        "id": format!("evt_{}", id.trim_start_matches("job_")),
        "type": kind,
        "created_at": chrono::Utc::now(),
        "data": {"job": JobView::from(job), "outputs": outputs, "expires_in": expires_in},
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
    let secret = account_secret(master, &row.user_id);
    let response = client
        .post(url)
        .header("content-type", "application/json")
        .header("convt-event-id", event["id"].as_str().unwrap_or_default())
        .header(
            "convt-event-type",
            event["type"].as_str().unwrap_or_default(),
        )
        .header("convt-delivery-attempt", row.webhook_attempts.to_string())
        .header("convt-signature", signature(&secret, now, &body))
        .body(body)
        .send()
        .await?;
    anyhow::ensure!(
        response.status().is_success(),
        "receiver answered HTTP {}",
        response.status()
    );
    Ok(())
}

/// Delivers webhooks every two seconds until the process exits.
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
        assert_eq!(
            account_secret("master-secret-for-tests-only-0000", "usr_a"),
            "whsec_8d51d0e89e4893fe0d9bf302df9badafa854a3119b513ada671d8a744984351d"
        );
        assert_ne!(
            account_secret("master-secret-for-tests-only-0000", "usr_a"),
            account_secret("master-secret-for-tests-only-0000", "usr_b")
        );
        assert_eq!(
            signature("whsec_test", 1_700_000_000, br#"{"ok":true}"#),
            "t=1700000000,v1=85876387ad9d6be57a04653bc0729da757049f58afb10ba6cac3bedaecf4fda3"
        );
    }
}
