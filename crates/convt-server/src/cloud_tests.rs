use crate::{
    jobs::{self, CreateJob, Principal},
    meter::{self, Meter, MeterEvent},
    routes::{self, CloudState},
    storage::Storage,
    testing::test_db,
};
use async_trait::async_trait;
use axum::{
    body::{Body, to_bytes},
    http::{Request, StatusCode},
};
use std::{
    collections::{BTreeMap, HashSet},
    path::Path,
    sync::{Arc, Mutex},
};
use tower::ServiceExt;
const KEY: &str = "cvt_live_0123456789abcdefghjkmnpqrstvwxyz";
#[derive(Default)]
struct MemoryStorage {
    objects: Mutex<BTreeMap<String, i64>>,
}
#[async_trait]
impl Storage for MemoryStorage {
    async fn upload_url(&self, key: &str, _bytes: i64) -> anyhow::Result<String> {
        Ok(format!("https://upload.test/{key}"))
    }
    async fn download_url(&self, key: &str, _seconds: u32) -> anyhow::Result<String> {
        Ok(format!("https://download.test/{key}"))
    }
    async fn size(&self, key: &str) -> anyhow::Result<i64> {
        self.objects
            .lock()
            .unwrap()
            .get(key)
            .copied()
            .ok_or_else(|| anyhow::anyhow!("not uploaded"))
    }
    async fn seal(&self, from: &str, to: &str) -> anyhow::Result<()> {
        let bytes = self.size(from).await?;
        self.objects.lock().unwrap().insert(to.into(), bytes);
        Ok(())
    }
    async fn download(&self, _key: &str, _path: &Path) -> anyhow::Result<()> {
        Ok(())
    }
    async fn upload(&self, _key: &str, _path: &Path) -> anyhow::Result<()> {
        Ok(())
    }
    async fn delete(&self, key: &str) -> anyhow::Result<()> {
        self.objects.lock().unwrap().remove(key);
        Ok(())
    }
    async fn delete_prefix(&self, prefix: &str) -> anyhow::Result<()> {
        self.objects
            .lock()
            .unwrap()
            .retain(|k, _| !k.starts_with(prefix));
        Ok(())
    }
}
async fn setup(pool: &sqlx::PgPool, user: &str, kind: &str, cap: i32) {
    sqlx::query("insert into users (id,name,email,email_verified) values ($1,'',$2,true)")
        .bind(user)
        .bind(format!("{user}@convt.test"))
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("insert into subscriptions (id,user_id,email,kind,interval,status,provider_subscription_id,current_period_start,current_period_end,spend_cap_cents,card_seen_at) values ($1,$2,$3,$4,$5,'active',$1,date_trunc('month',now()),now()+interval '1 year',$6,now())").bind(format!("sub_{user}")).bind(user).bind(format!("{user}@convt.test")).bind(kind).bind(if kind=="pro"{Some("year")}else{None}).bind(if kind=="api"{Some(cap)}else{None}).execute(pool).await.unwrap();
}
fn who(user: &str, api: bool) -> Principal {
    Principal {
        user_id: user.into(),
        key_id: api.then(|| "key_a".into()),
    }
}
fn data(bytes: i64) -> CreateJob {
    CreateJob {
        input_format: "svg".into(),
        target_format: "png".into(),
        input_bytes: bytes,
    }
}
async fn key(pool: &sqlx::PgPool) {
    sqlx::query("insert into api_keys (id,user_id,name,prefix,secret_hash) values ('key_a','usr_a','test',$1,$2)").bind(&KEY[..17]).bind(&crate::api_keys::hash_key(KEY)[..]).execute(pool).await.unwrap();
}
async fn call(state: &CloudState, path: &str, method: &str) -> (StatusCode, serde_json::Value) {
    let r = routes::router(state.clone())
        .oneshot(
            Request::builder()
                .uri(path)
                .method(method)
                .header("authorization", format!("Bearer {KEY}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let status = r.status();
    let body = to_bytes(r.into_body(), 1_000_000).await.unwrap();
    (status, serde_json::from_slice(&body).unwrap())
}
#[tokio::test]
async fn cross_account_read_download_cancel_and_start_are_denied() {
    let Some(db) = test_db().await else { return };
    setup(&db.owner, "usr_a", "api", 100).await;
    key(&db.owner).await;
    setup(&db.owner, "usr_b", "pro", 0).await;
    let job = jobs::create(&db.server, &who("usr_b", false), &data(10))
        .await
        .unwrap();
    let state = CloudState {
        pool: db.server.clone(),
        storage: Arc::new(MemoryStorage::default()),
        web_secret: "s".repeat(32),
    };
    for (suffix, method) in [
        ("", "GET"),
        ("/download", "GET"),
        ("/start", "POST"),
        ("/cancel", "POST"),
    ] {
        assert_eq!(
            call(&state, &format!("/v1/jobs/{}{suffix}", job.id), method)
                .await
                .0,
            StatusCode::NOT_FOUND
        );
    }
    db.drop().await;
}
#[tokio::test]
async fn stale_worker_cannot_finish_after_replacement_and_metering_is_atomic() {
    let Some(db) = test_db().await else { return };
    setup(&db.owner, "usr_a", "api", 100).await;
    key(&db.owner).await;
    let job = jobs::create(&db.server, &who("usr_a", true), &data(10))
        .await
        .unwrap();
    sqlx::query("update cloud_jobs set status='queued',queued_at=now() where id=$1")
        .bind(&job.id)
        .execute(&db.owner)
        .await
        .unwrap();
    let old = jobs::claim(&db.server, "old").await.unwrap().unwrap();
    assert!(
        jobs::attempt_is_live(&db.server, &old.id, old.attempt)
            .await
            .unwrap()
    );
    sqlx::query("update cloud_jobs set lease_expires_at=now()-interval '1 second' where id=$1")
        .bind(&job.id)
        .execute(&db.owner)
        .await
        .unwrap();
    assert!(
        !jobs::attempt_is_live(&db.server, &old.id, old.attempt)
            .await
            .unwrap()
    );
    let replacement = jobs::claim(&db.server, "new").await.unwrap().unwrap();
    assert_eq!(replacement.attempt, old.attempt + 1);
    let keys = vec![format!(
        "{}/attempt-{}/output.png",
        job.id, replacement.attempt
    )];
    assert!(
        jobs::finish(&db.server, &replacement, &keys, None)
            .await
            .unwrap()
    );
    assert!(
        !jobs::finish(&db.server, &old, &["stale.png".into()], None)
            .await
            .unwrap()
    );
    assert!(
        !jobs::finish(&db.server, &replacement, &keys, None)
            .await
            .unwrap()
    );
    assert_eq!(
        jobs::owned(&db.server, &who("usr_a", true), &job.id)
            .await
            .unwrap()
            .output_keys
            .0,
        keys
    );
    let n: i64 = sqlx::query_scalar("select count(*) from usage_events where job_id=$1")
        .bind(&job.id)
        .fetch_one(&db.owner)
        .await
        .unwrap();
    assert_eq!(n, 1);
    db.drop().await;
}
#[tokio::test]
async fn concurrent_api_reservations_stop_at_cap_and_lowering_preserves_commitments() {
    let Some(db) = test_db().await else { return };
    setup(&db.owner, "usr_a", "api", 100).await;
    key(&db.owner).await;
    sqlx::query("insert into usage_events (id,user_id,subscription_id,job_id,kind,quantity,amount_cents,occurred_at) values ('use_old','usr_a','sub_usr_a','old','api_conversion',98,98,now())").execute(&db.owner).await.unwrap();
    let mut tasks = tokio::task::JoinSet::new();
    for _ in 0..10 {
        let pool = db.server.clone();
        tasks.spawn(async move {
            jobs::create(&pool, &who("usr_a", true), &data(10))
                .await
                .is_ok()
        });
    }
    let mut accepted = 0;
    while let Some(result) = tasks.join_next().await {
        accepted += usize::from(result.unwrap());
    }
    assert_eq!(accepted, 2);
    sqlx::query("update subscriptions set spend_cap_cents=99 where id='sub_usr_a'")
        .execute(&db.owner)
        .await
        .unwrap();
    assert!(
        jobs::create(&db.server, &who("usr_a", true), &data(10))
            .await
            .is_err()
    );
    let open: i64 = sqlx::query_scalar(
        "select sum(reserved_cents)::bigint from cloud_jobs where reservation='open'",
    )
    .fetch_one(&db.owner)
    .await
    .unwrap();
    assert_eq!(open, 2);
    db.drop().await;
}
#[tokio::test]
async fn annual_pro_has_monthly_bytes_and_rejects_trial_and_oversized_files() {
    let Some(db) = test_db().await else { return };
    setup(&db.owner, "usr_a", "pro", 0).await;
    assert!(
        jobs::create(
            &db.server,
            &who("usr_a", false),
            &data(jobs::MAX_FILE_BYTES + 1)
        )
        .await
        .is_err()
    );
    sqlx::query("insert into usage_events (id,user_id,subscription_id,job_id,kind,quantity,amount_cents,occurred_at) values ('use_old','usr_a','sub_usr_a','old','pro_bytes',49999999999,0,now())").execute(&db.owner).await.unwrap();
    assert!(
        jobs::create(&db.server, &who("usr_a", false), &data(2))
            .await
            .is_err()
    );
    assert!(
        jobs::create(&db.server, &who("usr_a", false), &data(1))
            .await
            .is_ok()
    );
    sqlx::query("update subscriptions set status='trialing' where id='sub_usr_a'")
        .execute(&db.owner)
        .await
        .unwrap();
    assert!(
        jobs::create(&db.server, &who("usr_a", false), &data(1))
            .await
            .is_err()
    );
    db.drop().await;
}
#[tokio::test]
async fn actual_size_mismatch_releases_reservation_and_expiry_fences_completion() {
    let Some(db) = test_db().await else { return };
    setup(&db.owner, "usr_a", "api", 100).await;
    key(&db.owner).await;
    let who = who("usr_a", true);
    let job = jobs::create(&db.server, &who, &data(10)).await.unwrap();
    let storage = Arc::new(MemoryStorage::default());
    storage
        .objects
        .lock()
        .unwrap()
        .insert(job.input_key.clone().unwrap(), 11);
    let state = CloudState {
        pool: db.server.clone(),
        storage: storage.clone(),
        web_secret: "s".repeat(32),
    };
    assert_eq!(
        call(&state, &format!("/v1/jobs/{}/start", job.id), "POST")
            .await
            .0,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        jobs::owned(&db.server, &who, &job.id)
            .await
            .unwrap()
            .reservation,
        "released"
    );
    let job = jobs::create(&db.server, &who, &data(10)).await.unwrap();
    sqlx::query("update cloud_jobs set status='queued' where id=$1")
        .bind(&job.id)
        .execute(&db.owner)
        .await
        .unwrap();
    let running = jobs::claim(&db.server, "worker").await.unwrap().unwrap();
    sqlx::query("update cloud_jobs set expires_at=now()-interval '1 second' where id=$1")
        .bind(&job.id)
        .execute(&db.owner)
        .await
        .unwrap();
    routes::expire(&state).await.unwrap();
    assert!(
        !jobs::finish(&db.server, &running, &["out".into()], None)
            .await
            .unwrap()
    );
    assert!(
        storage
            .objects
            .lock()
            .unwrap()
            .keys()
            .all(|k| !k.starts_with(&job.id))
    );
    db.drop().await;
}
#[derive(Default)]
struct RetryMeter {
    seen: Mutex<HashSet<String>>,
    fail: Mutex<bool>,
}
#[async_trait]
impl Meter for RetryMeter {
    async fn send(&self, e: &MeterEvent) -> anyhow::Result<()> {
        self.seen.lock().unwrap().insert(e.job_id.clone());
        let mut fail = self.fail.lock().unwrap();
        if *fail {
            *fail = false;
            anyhow::bail!("accepted but response lost");
        }
        Ok(())
    }
}
#[tokio::test]
async fn meter_retry_uses_job_id_after_acceptance_and_response_loss() {
    let Some(db) = test_db().await else { return };
    setup(&db.owner, "usr_a", "api", 100).await;
    sqlx::query("insert into usage_events (id,user_id,subscription_id,job_id,kind,quantity,amount_cents,occurred_at) values ('use_once','usr_a','sub_usr_a','job_once','api_conversion',1,1,now())").execute(&db.owner).await.unwrap();
    let meter = RetryMeter::default();
    *meter.fail.lock().unwrap() = true;
    assert!(meter::drain(&db.server, &meter).await.is_err());
    assert_eq!(meter::drain(&db.server, &meter).await.unwrap(), 1);
    assert_eq!(meter::drain(&db.server, &meter).await.unwrap(), 0);
    assert_eq!(meter.seen.lock().unwrap().len(), 1);
    db.drop().await;
}

#[tokio::test]
async fn polar_http_sender_uses_versioned_external_id_and_validates_acknowledgments() {
    use crate::meter::PolarMeter;
    use axum::{Json, Router, http::HeaderMap, routing::post};
    let seen = Arc::new(Mutex::new(HashSet::new()));
    let captured = seen.clone();
    let app=Router::new().route("/v1/events/ingest",post(move |headers:HeaderMap,Json(body):Json<serde_json::Value>| {let seen=captured.clone();async move{
        assert_eq!(headers["polar-version"],"2026-10");assert_eq!(headers["authorization"],"Bearer local-test-token");
        let event=&body["events"][0];assert_eq!(event["name"],"api_conversion");assert_eq!(event["external_customer_id"],"usr_a");assert!(event.get("identifier").is_none());assert_eq!(event["metadata"]["quantity"],1);
        let inserted=seen.lock().unwrap().insert(event["external_id"].as_str().unwrap().to_owned());
        Json(serde_json::json!({"inserted":usize::from(inserted),"duplicates":usize::from(!inserted)}))
    }}));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    let meter = PolarMeter {
        endpoint: format!("http://{address}"),
        token: "local-test-token".into(),
    };
    let e = MeterEvent {
        id: "use_one".into(),
        job_id: "job_one".into(),
        user_id: "usr_a".into(),
        quantity: 1,
        occurred_at: chrono::Utc::now(),
    };
    meter.send(&e).await.unwrap();
    meter.send(&e).await.unwrap();
    assert_eq!(seen.lock().unwrap().len(), 1);
    server.abort();
    let _ = server.await;
}
#[test]
fn web_tokens_expire_and_cannot_be_changed_to_another_account() {
    use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
    use hmac::{Hmac, Mac};
    use sha2::Sha256;
    let secret = "s".repeat(32);
    let sign = |sub: &str, exp: i64| {
        let body = URL_SAFE_NO_PAD.encode(
            serde_json::to_vec(&crate::tokens::Claims {
                sub: sub.into(),
                exp,
                aud: "convt-cloud-web".into(),
            })
            .unwrap(),
        );
        let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).unwrap();
        mac.update(body.as_bytes());
        format!(
            "cvt_web_{body}.{}",
            hex::encode(mac.finalize().into_bytes())
        )
    };
    let now = chrono::Utc::now().timestamp();
    assert_eq!(
        crate::tokens::verify(&secret, &sign("usr_a", now + 60)),
        Some("usr_a".into())
    );
    assert_eq!(
        crate::tokens::verify(&secret, &sign("usr_a", now - 1)),
        None
    );
    assert_eq!(
        crate::tokens::verify(&secret, &sign("usr_a", now + 360)),
        None
    );
    let a = sign("usr_a", now + 60);
    let b = sign("usr_b", now + 60);
    let forged = format!(
        "{}.{}",
        b.split('.').next().unwrap(),
        a.split('.').nth(1).unwrap()
    );
    assert_eq!(crate::tokens::verify(&secret, &forged), None);
}

#[tokio::test]
async fn corrections_count_toward_the_transactional_cap() {
    let Some(db) = test_db().await else { return };
    setup(&db.owner, "usr_a", "api", 2).await;
    key(&db.owner).await;
    sqlx::query("insert into usage_events (id,user_id,subscription_id,kind,quantity,amount_cents,occurred_at) values ('original','usr_a','sub_usr_a','api_conversion',1,1,now())").execute(&db.owner).await.unwrap();
    sqlx::query("insert into usage_events (id,user_id,subscription_id,kind,quantity,amount_cents,corrects,occurred_at) values ('adjustment','usr_a','sub_usr_a','correction',0,1,'original',now())").execute(&db.owner).await.unwrap();
    assert!(
        jobs::create(&db.server, &who("usr_a", true), &data(10))
            .await
            .is_err()
    );
    db.drop().await;
}

#[tokio::test]
async fn cancelled_upload_urls_remain_in_the_storage_budget_until_expiry() {
    let Some(db) = test_db().await else { return };
    setup(&db.owner, "usr_a", "api", 100).await;
    key(&db.owner).await;
    let principal = who("usr_a", true);
    // No object needs to be sent: all these signed URLs can still accept 2 GB.
    for _ in 0..25 {
        let job = jobs::create(&db.server, &principal, &data(jobs::MAX_FILE_BYTES))
            .await
            .unwrap();
        jobs::cancel(&db.server, &principal, &job.id).await.unwrap();
    }
    assert!(
        jobs::create(&db.server, &principal, &data(jobs::MAX_FILE_BYTES))
            .await
            .is_err(),
        "cancellation must not release outstanding storage commitments"
    );
    sqlx::query("update cloud_jobs set expires_at=now()-interval '1 second' where user_id='usr_a'")
        .execute(&db.owner)
        .await
        .unwrap();
    assert!(
        jobs::create(&db.server, &principal, &data(jobs::MAX_FILE_BYTES))
            .await
            .is_err(),
        "expiry alone does not release storage before cleanup"
    );
    let state = CloudState {
        pool: db.server.clone(),
        storage: Arc::new(MemoryStorage::default()),
        web_secret: "s".repeat(32),
    };
    routes::expire(&state).await.unwrap();
    assert!(
        jobs::create(&db.server, &principal, &data(jobs::MAX_FILE_BYTES))
            .await
            .is_ok()
    );
    db.drop().await;
}

#[tokio::test]
async fn api_and_pro_share_a_serialized_storage_budget() {
    let Some(db) = test_db().await else { return };
    setup(&db.owner, "usr_a", "api", 100).await;
    key(&db.owner).await;
    sqlx::query("insert into subscriptions (id,user_id,email,kind,interval,status,provider_subscription_id,current_period_start,current_period_end) values ('pro_storage','usr_a','usr_a@convt.test','pro','month','active','pro_storage',now()-interval '1 day',now()+interval '1 month')").execute(&db.owner).await.unwrap();
    for _ in 0..24 {
        let j = jobs::create(&db.server, &who("usr_a", true), &data(jobs::MAX_FILE_BYTES))
            .await
            .unwrap();
        jobs::cancel(&db.server, &who("usr_a", true), &j.id)
            .await
            .unwrap();
    }
    let api = who("usr_a", true);
    let pro = who("usr_a", false);
    let input = data(jobs::MAX_FILE_BYTES);
    let (a, b) = tokio::join!(
        jobs::create(&db.server, &api, &input),
        jobs::create(&db.server, &pro, &input)
    );
    assert_eq!(usize::from(a.is_ok()) + usize::from(b.is_ok()), 1);
    db.drop().await;
}
