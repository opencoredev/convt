//! At-least-once sender; Polar's event identifier is always the immutable job id.
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::{FromRow, PgPool};
#[derive(FromRow, Serialize, Clone, Debug)]
pub struct MeterEvent {
    pub id: String,
    pub job_id: String,
    pub user_id: String,
    pub quantity: i64,
    pub occurred_at: DateTime<Utc>,
}
#[async_trait]
pub trait Meter: Send + Sync {
    async fn send(&self, event: &MeterEvent) -> anyhow::Result<()>;
}
pub struct PolarMeter {
    pub endpoint: String,
    pub token: String,
}
#[async_trait]
impl Meter for PolarMeter {
    async fn send(&self, e: &MeterEvent) -> anyhow::Result<()> {
        let response=reqwest::Client::new().post(format!("{}/v1/events/ingest",self.endpoint.trim_end_matches('/'))).bearer_auth(&self.token).header("Polar-Version","2026-10").timeout(std::time::Duration::from_secs(15)).json(&serde_json::json!({"events":[{"name":"api_conversion","external_customer_id":e.user_id,"timestamp":e.occurred_at.to_rfc3339(),"metadata":{"quantity":e.quantity},"external_id":e.job_id}]})).send().await?;
        anyhow::ensure!(
            response.status().is_success(),
            "meter HTTP {}",
            response.status()
        );
        let receipt: serde_json::Value = response.json().await?;
        anyhow::ensure!(
            receipt["inserted"].as_u64().unwrap_or(0) + receipt["duplicates"].as_u64().unwrap_or(0)
                == 1,
            "meter did not acknowledge this event"
        );
        Ok(())
    }
}
pub async fn drain(pool: &PgPool, meter: &dyn Meter) -> anyhow::Result<usize> {
    let events=sqlx::query_as!(MeterEvent, r#"select e.id,e.job_id as "job_id!",e.user_id as "user_id!",e.quantity,e.occurred_at from usage_events e join subscriptions s on s.id=e.subscription_id where e.kind='api_conversion' and e.reported_at is null and e.user_id is not null and s.provider='polar' order by e.occurred_at limit 100"#).fetch_all(pool).await?;
    let mut sent = 0;
    for e in events {
        meter.send(&e).await?;
        sqlx::query!(r#"update usage_events set reported_at=now(),provider_event_id=$2 where id=$1 and reported_at is null"#, &e.id, &e.job_id).execute(pool).await?;
        sent += 1;
    }
    Ok(sent)
}
