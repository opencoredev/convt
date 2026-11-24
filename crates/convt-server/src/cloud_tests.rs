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
    data: Mutex<BTreeMap<String, Vec<u8>>>,
}
#[async_trait]
impl Storage for MemoryStorage {
    async fn upload_url(&self, key: &str, _bytes: i64, _seconds: u32) -> anyhow::Result<String> {
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
    async fn check(&self) -> anyhow::Result<()> {
        Ok(())
    }
    async fn head(&self, key: &str) -> anyhow::Result<Option<i64>> {
        Ok(self.objects.lock().unwrap().get(key).copied())
    }
    async fn put(&self, key: &str, bytes: axum::body::Bytes) -> anyhow::Result<()> {
        self.objects
            .lock()
            .unwrap()
            .insert(key.into(), bytes.len() as i64);
        self.data.lock().unwrap().insert(key.into(), bytes.to_vec());
        Ok(())
    }
    async fn open(&self, key: &str) -> anyhow::Result<crate::storage::ByteStream> {
        let bytes = self
            .data
            .lock()
            .unwrap()
            .get(key)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("missing"))?;
        Ok(Box::pin(futures_util::stream::once(async move {
            Ok(axum::body::Bytes::from(bytes))
        })))
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
        input_bytes: Some(bytes),
        options: Default::default(),
        source_url: None,
        webhook_url: None,
        input_sha256: None,
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
async fn create_call(state: &CloudState, body: &str) -> (StatusCode, serde_json::Value) {
    let r = routes::router(state.clone())
        .oneshot(
            Request::builder()
                .uri("/v1/jobs")
                .method("POST")
                .header("authorization", format!("Bearer {KEY}"))
                .header("content-type", "application/json")
                .body(Body::from(body.to_owned()))
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
        webhook_secret: None,
        source_urls: true,
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
async fn a_desktop_and_cloud_year_shares_the_accounts_monthly_allowance() {
    let Some(db) = test_db().await else { return };
    // A Pro subscriber who also bought Desktop + Cloud: two usable Pro rows.
    setup(&db.owner, "usr_b", "pro", 0).await;
    sqlx::query("insert into orders (id,provider,provider_order_id,user_id,email,product,amount_cents,currency,status,paid_at,billed_at) values ('ord_b','polar','polar_ord_b','usr_b','usr_b@convt.test','bundle',6900,'usd','paid',now(),now())").execute(&db.owner).await.unwrap();
    sqlx::query("insert into subscriptions (id,user_id,email,kind,interval,status,provider_subscription_id,current_period_start,current_period_end,cancel_at_period_end,ended_at,order_id,created_at) values ('cloud_b','usr_b','usr_b@convt.test','pro','year','active',null,now()-interval '1 day',now()+interval '364 days',true,now()+interval '364 days','ord_b',now()+interval '1 second')").execute(&db.owner).await.unwrap();
    // Usage on the subscription counts against the job that lands on the newer year.
    sqlx::query("insert into usage_events (id,user_id,subscription_id,job_id,kind,quantity,amount_cents,occurred_at) values ('use_b','usr_b','sub_usr_b','old_b','pro_bytes',49999999999,0,now())").execute(&db.owner).await.unwrap();
    assert!(
        jobs::create(&db.server, &who("usr_b", false), &data(2))
            .await
            .is_err()
    );
    let job = jobs::create(&db.server, &who("usr_b", false), &data(1))
        .await
        .unwrap();
    assert_eq!(job.subscription_id.as_deref(), Some("cloud_b"));
    // An open reservation on the year counts once the job lands on the subscription.
    setup(&db.owner, "usr_c", "pro", 0).await;
    sqlx::query("insert into orders (id,provider,provider_order_id,user_id,email,product,amount_cents,currency,status,paid_at,billed_at) values ('ord_c','polar','polar_ord_c','usr_c','usr_c@convt.test','bundle',6900,'usd','paid',now(),now())").execute(&db.owner).await.unwrap();
    sqlx::query("insert into subscriptions (id,user_id,email,kind,interval,status,provider_subscription_id,current_period_start,current_period_end,cancel_at_period_end,ended_at,order_id,created_at) values ('cloud_c','usr_c','usr_c@convt.test','pro','year','active',null,now()-interval '1 day',now()+interval '364 days',true,now()+interval '364 days','ord_c',now()+interval '1 second')").execute(&db.owner).await.unwrap();
    let held = jobs::create(&db.server, &who("usr_c", false), &data(1))
        .await
        .unwrap();
    assert_eq!(held.subscription_id.as_deref(), Some("cloud_c"));
    sqlx::query("update cloud_jobs set reserved_bytes=49999999999 where id=$1")
        .bind(&held.id)
        .execute(&db.owner)
        .await
        .unwrap();
    sqlx::query("update subscriptions set status='canceled' where id='cloud_c'")
        .execute(&db.owner)
        .await
        .unwrap();
    assert!(
        jobs::create(&db.server, &who("usr_c", false), &data(2))
            .await
            .is_err()
    );
    let next = jobs::create(&db.server, &who("usr_c", false), &data(1))
        .await
        .unwrap();
    assert_eq!(next.subscription_id.as_deref(), Some("sub_usr_c"));
    db.drop().await;
}
#[tokio::test]
async fn finishing_on_an_older_pro_row_waits_for_the_account_lock() {
    let Some(db) = test_db().await else { return };
    // A job reserved on the subscription, then a newer Desktop + Cloud year:
    // `create` now locks the year's row, `finish` the subscription's.
    setup(&db.owner, "usr_d", "pro", 0).await;
    let job = jobs::create(&db.server, &who("usr_d", false), &data(1))
        .await
        .unwrap();
    assert_eq!(job.subscription_id.as_deref(), Some("sub_usr_d"));
    sqlx::query("update cloud_jobs set status='queued',queued_at=now() where id=$1")
        .bind(&job.id)
        .execute(&db.owner)
        .await
        .unwrap();
    let running = jobs::claim(&db.server, "worker").await.unwrap().unwrap();
    sqlx::query("insert into orders (id,provider,provider_order_id,user_id,email,product,amount_cents,currency,status,paid_at,billed_at) values ('ord_d','polar','polar_ord_d','usr_d','usr_d@convt.test','bundle',6900,'usd','paid',now(),now())").execute(&db.owner).await.unwrap();
    sqlx::query("insert into subscriptions (id,user_id,email,kind,interval,status,provider_subscription_id,current_period_start,current_period_end,cancel_at_period_end,ended_at,order_id,created_at) values ('cloud_d','usr_d','usr_d@convt.test','pro','year','active',null,now()-interval '1 day',now()+interval '364 days',true,now()+interval '364 days','ord_d',now()+interval '1 second')").execute(&db.owner).await.unwrap();
    // Hold the account lock as `create` does between its settled and reserved reads.
    let mut held = db.owner.begin().await.unwrap();
    sqlx::query("select pg_advisory_xact_lock(hashtextextended('usr_d', 9))")
        .execute(&mut *held)
        .await
        .unwrap();
    let server = db.server.clone();
    let keys = vec![format!("{}/attempt-{}/output.png", job.id, running.attempt)];
    let finishing = tokio::spawn(async move { jobs::finish(&server, &running, &keys, None).await });
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    assert!(
        !finishing.is_finished(),
        "finish settled while create held the account lock"
    );
    held.commit().await.unwrap();
    assert!(finishing.await.unwrap().unwrap());
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
        webhook_secret: None,
        source_urls: true,
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
        webhook_secret: None,
        source_urls: true,
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
#[tokio::test]
async fn malformed_job_requests_get_json_errors() {
    let Some(db) = test_db().await else { return };
    setup(&db.owner, "usr_a", "api", 100).await;
    key(&db.owner).await;
    let state = CloudState {
        pool: db.server.clone(),
        storage: Arc::new(MemoryStorage::default()),
        web_secret: "s".repeat(32),
        webhook_secret: None,
        source_urls: true,
    };
    let with = |options: &str| {
        format!(
            r#"{{"input_format":"png","target_format":"jpeg","input_bytes":10,"options":{options}}}"#
        )
    };
    // Values serde refuses get the same code as values the range checks refuse.
    for options in [
        r#"{"quality":256}"#,
        r#"{"quality":150}"#,
        r#"{"pages":"3-1"}"#,
        r#"{"background":"plaid"}"#,
        r#"{"qualty":40}"#,
        r#""fast""#,
    ] {
        let (status, body) = create_call(&state, &with(options)).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{options}: {body}");
        assert_eq!(
            body["error"]["code"], "invalid_options",
            "{options}: {body}"
        );
    }
    let (_, body) = create_call(&state, &with(r#"{"quality":256}"#)).await;
    assert!(
        body["error"]["message"]
            .as_str()
            .unwrap()
            .contains("quality"),
        "{body}"
    );
    for request in [
        r#"{"input_format":"png","target_format":"jpeg"}"#,
        r#"{"input_format":"png","target_format":"jpeg","input_bytes":"ten"}"#,
        r#"{"input_format":"png","target_format":"jpeg","input_bytes":10,"extra":1}"#,
        r#"["png"]"#,
        "not json",
    ] {
        let (status, body) = create_call(&state, request).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{request}: {body}");
        assert_eq!(
            body["error"]["code"], "invalid_request",
            "{request}: {body}"
        );
    }
    let (status, body) = create_call(&state, &with(r#"{"quality":40}"#)).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["job"]["options"]["quality"], 40);
    db.drop().await;
}
#[test]
fn create_requests_take_optional_validated_options() {
    let plain: CreateJob = serde_json::from_value(
        serde_json::json!({"input_format":"svg","target_format":"png","input_bytes":10}),
    )
    .unwrap();
    assert_eq!(plain.options, convt_core::Options::default());
    let with: CreateJob = serde_json::from_value(serde_json::json!({"input_format":"png","target_format":"jpeg","input_bytes":10,"options":{"quality":40,"max_size":256,"background":"black"}})).unwrap();
    assert_eq!(with.options.quality, Some(40));
    assert_eq!(with.options.max_size, Some(256));
    assert!(jobs::validate_options(&with.options, "jpeg").is_ok());
    for bad in [
        serde_json::json!({"qualty":40}),
        serde_json::json!({"quality":"high"}),
        serde_json::json!({"pages":"3-1"}),
        serde_json::json!({"video_codec":"vp8"}),
    ] {
        assert!(
            serde_json::from_value::<CreateJob>(serde_json::json!({"input_format":"png","target_format":"jpeg","input_bytes":10,"options":bad})).is_err(),
            "{bad}"
        );
    }
    let out_of_range = |o: serde_json::Value, to: &str| {
        jobs::validate_options(&serde_json::from_value(o).unwrap(), to).is_err()
    };
    assert!(out_of_range(serde_json::json!({"quality":0}), "jpeg"));
    assert!(out_of_range(serde_json::json!({"quality":101}), "jpeg"));
    assert!(out_of_range(serde_json::json!({"video_height":721}), "mp4"));
    assert!(out_of_range(serde_json::json!({"dpi":5}), "png"));
    assert!(out_of_range(
        serde_json::json!({"background":"transparent"}),
        "jpeg"
    ));
    assert!(!out_of_range(
        serde_json::json!({"background":"transparent"}),
        "png"
    ));
}
#[tokio::test]
async fn options_are_stored_returned_and_claimed() {
    let Some(db) = test_db().await else { return };
    setup(&db.owner, "usr_a", "pro", 0).await;
    let mut request = data(10);
    request.options.quality = Some(150);
    let refused = jobs::create(&db.server, &who("usr_a", false), &request)
        .await
        .unwrap_err();
    assert!(format!("{refused:?}").contains("invalid_options"));
    request.options.quality = Some(35);
    request.options.max_size = Some(64);
    let job = jobs::create(&db.server, &who("usr_a", false), &request)
        .await
        .unwrap();
    assert_eq!(job.options.0, request.options);
    let view = serde_json::to_value(routes::JobView::from(job.clone())).unwrap();
    assert_eq!(view["options"]["quality"], 35);
    assert_eq!(view["options"]["max_size"], 64);
    sqlx::query("update cloud_jobs set status='queued',queued_at=now() where id=$1")
        .bind(&job.id)
        .execute(&db.owner)
        .await
        .unwrap();
    let claimed = jobs::claim(&db.server, "w").await.unwrap().unwrap();
    assert_eq!(claimed.id, job.id);
    assert_eq!(claimed.options.0, request.options);
    // Rows written before options existed read as the defaults.
    let plain = jobs::create(&db.server, &who("usr_a", false), &data(10))
        .await
        .unwrap();
    assert_eq!(plain.options.0, convt_core::Options::default());
    db.drop().await;
}

// ---- Developer-experience routes: idempotency, /v1/convert, headers, webhooks ----

fn state_with(db: &crate::testing::TestDb, storage: Arc<MemoryStorage>) -> CloudState {
    CloudState {
        pool: db.server.clone(),
        storage,
        web_secret: "s".repeat(32),
        webhook_secret: Some("w".repeat(32)),
        source_urls: true,
    }
}

async fn send(
    state: &CloudState,
    request: Request<Body>,
) -> (StatusCode, axum::http::HeaderMap, Vec<u8>) {
    let r = crate::service(Some(routes::router(state.clone())))
        .oneshot(request)
        .await
        .unwrap();
    let status = r.status();
    let headers = r.headers().clone();
    let body = to_bytes(r.into_body(), 50_000_000).await.unwrap().to_vec();
    (status, headers, body)
}

/// A web credential as the site Worker mints them (see tokens.rs).
fn web_token(secret: &str, user: &str) -> String {
    use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
    use hmac::{Hmac, Mac};
    let claims = serde_json::json!({"sub": user, "exp": chrono::Utc::now().timestamp() + 120, "aud": "convt-cloud-web"});
    let body = URL_SAFE_NO_PAD.encode(claims.to_string());
    let mut mac = Hmac::<sha2::Sha256>::new_from_slice(secret.as_bytes()).unwrap();
    mac.update(body.as_bytes());
    format!(
        "cvt_web_{body}.{}",
        hex::encode(mac.finalize().into_bytes())
    )
}

fn json(body: &[u8]) -> serde_json::Value {
    serde_json::from_slice(body)
        .unwrap_or_else(|_| serde_json::json!({"raw": String::from_utf8_lossy(body)}))
}

/// Plays a worker: claims the next job and finishes it with one output.
async fn work_one(pool: &sqlx::PgPool, storage: &MemoryStorage, bytes: &[u8]) -> String {
    for _ in 0..100 {
        if let Some(job) = jobs::claim(pool, "test-worker").await.unwrap() {
            let key = format!(
                "{}/attempt-{}/input.{}",
                job.id, job.attempt, job.target_format
            );
            storage
                .put(&key, axum::body::Bytes::copy_from_slice(bytes))
                .await
                .unwrap();
            assert!(jobs::finish(pool, &job, &[key], None).await.unwrap());
            return job.id;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    panic!("no job was queued");
}

fn authed(method: &str, uri: &str) -> axum::http::request::Builder {
    Request::builder()
        .method(method)
        .uri(uri)
        .header("authorization", format!("Bearer {KEY}"))
}

#[tokio::test]
async fn idempotency_key_replays_without_a_second_reservation() {
    let Some(db) = test_db().await else { return };
    setup(&db.owner, "usr_a", "api", 100).await;
    key(&db.owner).await;
    let state = state_with(&db, Arc::new(MemoryStorage::default()));
    let body = r#"{"input_format":"png","target_format":"webp","input_bytes":10}"#;
    let create = |key: &'static str, body: &'static str| {
        authed("POST", "/v1/jobs")
            .header("content-type", "application/json")
            .header("idempotency-key", key)
            .body(Body::from(body))
            .unwrap()
    };
    let (status, headers, first) = send(&state, create("order-42", body)).await;
    assert_eq!(status, StatusCode::OK, "{}", json(&first));
    assert!(headers.get("idempotent-replayed").is_none());
    assert_eq!(headers["x-ratelimit-limit"], "120");
    assert!(headers.contains_key("x-ratelimit-remaining"));
    assert!(
        headers["x-request-id"]
            .to_str()
            .unwrap()
            .starts_with("req_")
    );
    let (status, headers, second) = send(&state, create("order-42", body)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(headers["idempotent-replayed"], "true");
    assert_eq!(json(&first)["job"]["id"], json(&second)["job"]["id"]);
    assert!(json(&second)["upload_url"].is_string());
    let reserved: i64 = sqlx::query_scalar(
        "select count(*) from cloud_jobs where user_id='usr_a' and reservation='open'",
    )
    .fetch_one(&db.owner)
    .await
    .unwrap();
    assert_eq!(reserved, 1);
    let (status, _, conflict) = send(
        &state,
        create(
            "order-42",
            r#"{"input_format":"png","target_format":"jpeg","input_bytes":10}"#,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(json(&conflict)["error"]["code"], "idempotency_key_reused");
    let (status, _, bad) = send(
        &state,
        authed("POST", "/v1/jobs")
            .header("content-type", "application/json")
            .header("idempotency-key", "")
            .body(Body::from(body))
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(json(&bad)["error"]["code"], "invalid_idempotency_key");
    // An expired job frees its key for a new request.
    sqlx::query("update cloud_jobs set expires_at=now()-interval '1 second'")
        .execute(&db.owner)
        .await
        .unwrap();
    let (status, headers, third) = send(&state, create("order-42", body)).await;
    assert_eq!(status, StatusCode::OK);
    assert!(headers.get("idempotent-replayed").is_none());
    assert_ne!(json(&first)["job"]["id"], json(&third)["job"]["id"]);
    db.drop().await;
}

#[tokio::test]
async fn start_without_an_upload_says_so() {
    let Some(db) = test_db().await else { return };
    setup(&db.owner, "usr_a", "api", 100).await;
    key(&db.owner).await;
    let state = state_with(&db, Arc::new(MemoryStorage::default()));
    let job = jobs::create(&db.server, &who("usr_a", true), &data(10))
        .await
        .unwrap();
    let (status, _, body) = send(
        &state,
        authed("POST", &format!("/v1/jobs/{}/start", job.id))
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(json(&body)["error"]["code"], "upload_missing");
    assert_eq!(
        jobs::owned(&db.server, &who("usr_a", true), &job.id)
            .await
            .unwrap()
            .status,
        "created"
    );
    db.drop().await;
}

#[tokio::test]
async fn convert_returns_the_file_in_one_request() {
    let Some(db) = test_db().await else { return };
    setup(&db.owner, "usr_a", "api", 100).await;
    key(&db.owner).await;
    let storage = Arc::new(MemoryStorage::default());
    let state = state_with(&db, storage.clone());
    let worker = {
        let pool = db.server.clone();
        let storage = storage.clone();
        tokio::spawn(async move { work_one(&pool, &storage, b"RIFF-webp-bytes").await })
    };
    let boundary = "XyZ";
    let form = format!(
        "--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"holiday photo.png\"\r\nContent-Type: image/png\r\n\r\nPNG-fake\r\n--{boundary}\r\nContent-Disposition: form-data; name=\"to\"\r\n\r\nwebp\r\n--{boundary}\r\nContent-Disposition: form-data; name=\"options\"\r\n\r\n{{\"quality\":70}}\r\n--{boundary}--\r\n"
    );
    let (status, headers, body) = send(
        &state,
        authed("POST", "/v1/convert")
            .header(
                "content-type",
                format!("multipart/form-data; boundary={boundary}"),
            )
            .body(Body::from(form))
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{}", json(&body));
    assert_eq!(body, b"RIFF-webp-bytes");
    assert_eq!(headers["content-type"], "image/webp");
    assert_eq!(
        headers["content-disposition"],
        "attachment; filename=\"holiday photo.webp\""
    );
    let id = worker.await.unwrap();
    assert_eq!(headers["convt-job-id"], id.as_str());
    let job = jobs::owned(&db.server, &who("usr_a", true), &id)
        .await
        .unwrap();
    assert_eq!(job.options.0.quality, Some(70));
    assert_eq!(job.input_format, "png");
    assert_eq!(job.reservation, "settled");
    let billed: i64 = sqlx::query_scalar("select count(*) from usage_events where job_id=$1")
        .bind(&id)
        .fetch_one(&db.owner)
        .await
        .unwrap();
    assert_eq!(billed, 1);

    // A raw body with query parameters, asking for JSON.
    let worker = {
        let pool = db.server.clone();
        let storage = storage.clone();
        tokio::spawn(async move { work_one(&pool, &storage, b"jpeg").await })
    };
    let (status, _, body) = send(
        &state,
        authed("POST", "/v1/convert?to=jpg&from=png")
            .header("accept", "application/json")
            .body(Body::from("raw-png"))
            .unwrap(),
    )
    .await;
    worker.await.unwrap();
    assert_eq!(status, StatusCode::OK, "{}", json(&body));
    let body = json(&body);
    assert_eq!(body["job"]["status"], "succeeded");
    assert_eq!(body["job"]["target_format"], "jpeg");
    assert_eq!(body["outputs"].as_array().unwrap().len(), 1);
    db.drop().await;
}

#[tokio::test]
async fn convert_validates_and_can_return_early() {
    let Some(db) = test_db().await else { return };
    setup(&db.owner, "usr_a", "api", 100).await;
    key(&db.owner).await;
    let state = state_with(&db, Arc::new(MemoryStorage::default()));
    for (uri, body, code) in [
        ("/v1/convert?from=png", "x", "invalid_request"),
        ("/v1/convert?to=webp", "x", "input_format_required"),
        ("/v1/convert?to=nope&from=png", "x", "unsupported_format"),
        (
            "/v1/convert?to=webp&from=png&wait=301",
            "x",
            "invalid_request",
        ),
        ("/v1/convert?to=webp&from=png", "", "file_too_large"),
        (
            "/v1/convert?to=webp&from=png&options=%7B%22quality%22%3A900%7D",
            "x",
            "invalid_options",
        ),
    ] {
        let (status, headers, response) =
            send(&state, authed("POST", uri).body(Body::from(body)).unwrap()).await;
        assert_eq!(
            status,
            StatusCode::BAD_REQUEST,
            "{uri}: {}",
            json(&response)
        );
        assert_eq!(json(&response)["error"]["code"], code, "{uri}");
        assert!(headers.contains_key("x-request-id"));
    }
    let (status, _, response) = send(
        &state,
        authed("POST", "/v1/convert?to=webp&from=png")
            .body(Body::from(vec![0_u8; crate::convert::MAX_INLINE_BYTES + 1]))
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::PAYLOAD_TOO_LARGE);
    assert_eq!(json(&response)["error"]["code"], "file_too_large");
    // A file request without a key that can't finish in time is cancelled.
    let (status, _, response) = send(
        &state,
        authed("POST", "/v1/convert?to=webp&from=png&wait=0")
            .body(Body::from("png"))
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::GATEWAY_TIMEOUT);
    assert_eq!(json(&response)["error"]["code"], "timeout");
    assert_eq!(json(&response)["job"]["status"], "cancelled");
    // With a key it keeps running and says so without a 2xx, so curl -f writes nothing.
    let (status, _, response) = send(
        &state,
        authed("POST", "/v1/convert?to=webp&from=png&wait=0")
            .header("idempotency-key", "slow-1")
            .body(Body::from("png"))
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(json(&response)["error"]["code"], "still_running");
    assert_eq!(json(&response)["job"]["status"], "queued");
    // A JSON caller gets 202 and the job keeps running.
    let (status, headers, response) = send(
        &state,
        authed("POST", "/v1/convert?to=webp&from=png&wait=0")
            .header("accept", "application/json")
            .body(Body::from("png"))
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::ACCEPTED);
    let job = &json(&response)["job"];
    assert_eq!(job["status"], "queued");
    assert_eq!(
        headers["location"],
        format!("/v1/jobs/{}", job["id"].as_str().unwrap()).as_str()
    );
    db.drop().await;
}

#[tokio::test]
async fn hanging_up_on_convert_cancels_the_job_unless_a_key_was_sent() {
    let Some(db) = test_db().await else { return };
    setup(&db.owner, "usr_a", "api", 100).await;
    key(&db.owner).await;
    let state = state_with(&db, Arc::new(MemoryStorage::default()));
    for with_key in [false, true] {
        let mut request = authed("POST", "/v1/convert?to=webp&from=png");
        if with_key {
            request = request.header("idempotency-key", "retry-me");
        }
        let app = crate::service(Some(routes::router(state.clone())));
        let request = request.body(Body::from("png")).unwrap();
        // The client gives up after 300 ms; no worker ever picks the job up.
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(300), app.oneshot(request))
                .await
                .is_err()
        );
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        let status: String = sqlx::query_scalar(
            "select status from cloud_jobs where user_id='usr_a' order by created_at desc limit 1",
        )
        .fetch_one(&db.owner)
        .await
        .unwrap();
        assert_eq!(status, if with_key { "queued" } else { "cancelled" });
    }
    db.drop().await;
}

#[tokio::test]
async fn urls_are_checked_and_webhooks_need_the_server_secret() {
    let Some(db) = test_db().await else { return };
    setup(&db.owner, "usr_a", "api", 100).await;
    key(&db.owner).await;
    let mut state = state_with(&db, Arc::new(MemoryStorage::default()));
    for (body, code) in [
        (
            r#"{"target_format":"webp","source_url":"https://169.254.169.254/latest/x.png"}"#,
            "invalid_source_url",
        ),
        (
            r#"{"target_format":"webp","source_url":"http://example.com/x.png"}"#,
            "invalid_source_url",
        ),
        (
            r#"{"input_format":"png","target_format":"webp","input_bytes":4,"source_url":"https://example.com/x.png"}"#,
            "invalid_request",
        ),
        (
            r#"{"input_format":"png","target_format":"webp","input_bytes":4,"webhook_url":"https://10.1.2.3/hook"}"#,
            "invalid_webhook_url",
        ),
    ] {
        let (status, _, response) = send(
            &state,
            authed("POST", "/v1/jobs")
                .header("content-type", "application/json")
                .body(Body::from(body))
                .unwrap(),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
        assert_eq!(json(&response)["error"]["code"], code, "{body}");
    }
    let (status, _, response) = send(
        &state,
        authed("POST", "/v1/jobs")
            .header("content-type", "application/json")
            .body(Body::from(
                r#"{"target_format":"webp","source_url":"https://cdn.example.com/a/b/photo.PNG?sig=1"}"#,
            ))
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{}", json(&response));
    let response = json(&response);
    assert_eq!(response["job"]["status"], "queued");
    assert_eq!(response["job"]["input_format"], "png");
    assert!(response["upload_url"].is_null());
    let (status, headers, secret) = send(
        &state,
        authed("GET", "/v1/webhooks/secret")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(headers["cache-control"], "no-store");
    assert_eq!(
        json(&secret)["secret"],
        crate::webhooks::account_secret(&"w".repeat(32), "usr_a", 1).as_str()
    );
    let (status, _, rotated) = send(
        &state,
        authed("POST", "/v1/webhooks/secret/rotate")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json(&rotated)["version"], 2);
    assert_ne!(json(&rotated)["secret"], json(&secret)["secret"]);
    // The browser converter's five-minute credential can't read the secret.
    let web = web_token(&state.web_secret, "usr_a");
    let (status, _, refused) = send(
        &state,
        Request::get("/v1/webhooks/secret")
            .header("authorization", format!("Bearer {web}"))
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(json(&refused)["error"]["code"], "api_key_required");
    // A server that hasn't switched source_url on refuses it.
    let mut off = state.clone();
    off.source_urls = false;
    let (status, _, refused) = send(
        &off,
        authed("POST", "/v1/jobs")
            .header("content-type", "application/json")
            .body(Body::from(
                r#"{"target_format":"webp","source_url":"https://cdn.example.com/a.png"}"#,
            ))
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(json(&refused)["error"]["code"], "source_url_unavailable");
    // An undecodable job id is a JSON 404, not plain text.
    let (status, headers, missing) = send(
        &state,
        authed("GET", "/v1/jobs/%FF%FE")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(headers["content-type"], "application/json");
    assert_eq!(json(&missing)["error"]["code"], "not_found");
    state.webhook_secret = None;
    let (status, _, response) = send(
        &state,
        authed("POST", "/v1/jobs")
            .header("content-type", "application/json")
            .body(Body::from(
                r#"{"input_format":"png","target_format":"webp","input_bytes":4,"webhook_url":"https://example.com/hook"}"#,
            ))
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(json(&response)["error"]["code"], "webhooks_unavailable");
    db.drop().await;
}

#[tokio::test]
async fn finished_jobs_deliver_one_signed_webhook() {
    let Some(db) = test_db().await else { return };
    setup(&db.owner, "usr_a", "api", 100).await;
    key(&db.owner).await;
    let storage = Arc::new(MemoryStorage::default());
    // A receiver that fails the first delivery and accepts the second.
    let received = Arc::new(Mutex::new(Vec::<(axum::http::HeaderMap, Vec<u8>)>::new()));
    let receiver = {
        let received = received.clone();
        axum::Router::new().route(
            "/hook",
            axum::routing::post(
                move |headers: axum::http::HeaderMap, body: axum::body::Bytes| {
                    let received = received.clone();
                    async move {
                        let mut seen = received.lock().unwrap();
                        seen.push((headers, body.to_vec()));
                        if seen.len() == 1 {
                            StatusCode::INTERNAL_SERVER_ERROR
                        } else {
                            StatusCode::NO_CONTENT
                        }
                    }
                },
            ),
        )
    };
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move { axum::serve(listener, receiver).await.unwrap() });
    let hook = format!("http://127.0.0.1:{port}/hook");
    // Creation checks the URL strictly; this test inserts it past that check.
    let job = jobs::create(&db.server, &who("usr_a", true), &data(10))
        .await
        .unwrap();
    sqlx::query("update cloud_jobs set webhook_url=$2, status='queued' where id=$1")
        .bind(&job.id)
        .bind(&hook)
        .execute(&db.owner)
        .await
        .unwrap();
    let lax = crate::net_guard::Policy {
        allow_private: true,
    };
    let master = "w".repeat(32);
    // Nothing is due while the job runs.
    assert_eq!(
        crate::webhooks::deliver_due(&db.server, storage.as_ref(), &master, lax)
            .await
            .unwrap(),
        0
    );
    assert!(received.lock().unwrap().is_empty());
    work_one(&db.server, &storage, b"out").await;
    assert_eq!(
        crate::webhooks::deliver_due(&db.server, storage.as_ref(), &master, lax)
            .await
            .unwrap(),
        0
    );
    // The failure scheduled a retry ten seconds out; bring it forward.
    sqlx::query("update cloud_jobs set webhook_next_at=now() where id=$1")
        .bind(&job.id)
        .execute(&db.owner)
        .await
        .unwrap();
    assert_eq!(
        crate::webhooks::deliver_due(&db.server, storage.as_ref(), &master, lax)
            .await
            .unwrap(),
        1
    );
    assert_eq!(
        crate::webhooks::deliver_due(&db.server, storage.as_ref(), &master, lax)
            .await
            .unwrap(),
        0
    );
    let seen = received.lock().unwrap().clone();
    assert_eq!(seen.len(), 2);
    let (headers, body) = &seen[1];
    let signature = headers["convt-signature"].to_str().unwrap();
    let t: i64 = signature
        .strip_prefix("t=")
        .unwrap()
        .split(',')
        .next()
        .unwrap()
        .parse()
        .unwrap();
    assert_eq!(
        signature,
        crate::webhooks::signature(
            &crate::webhooks::account_secret(&master, "usr_a", 1),
            t,
            body
        )
    );
    assert_eq!(headers["convt-event-type"], "job.succeeded");
    assert_eq!(headers["convt-delivery-attempt"], "2");
    let event: serde_json::Value = serde_json::from_slice(body).unwrap();
    assert_eq!(event["type"], "job.succeeded");
    assert_eq!(event["data"]["job"]["id"], job.id.as_str());
    assert_eq!(event["data"]["outputs"].as_array().unwrap().len(), 1);
    assert_eq!(
        headers["convt-event-id"].to_str().unwrap(),
        event["id"].as_str().unwrap()
    );
    db.drop().await;
}

#[tokio::test]
async fn expiry_keeps_finished_statuses_and_skips_settled_rows() {
    let Some(db) = test_db().await else { return };
    setup(&db.owner, "usr_a", "api", 100).await;
    key(&db.owner).await;
    let who = who("usr_a", true);
    let cancelled = jobs::create(&db.server, &who, &data(10)).await.unwrap();
    jobs::cancel(&db.server, &who, &cancelled.id).await.unwrap();
    let open = jobs::create(&db.server, &who, &data(10)).await.unwrap();
    sqlx::query("update cloud_jobs set expires_at=now()-interval '1 second'")
        .execute(&db.owner)
        .await
        .unwrap();
    let state = state_with(&db, Arc::new(MemoryStorage::default()));
    routes::expire(&state).await.unwrap();
    let rows: Vec<(String, String, String)> =
        sqlx::query_as("select id, status, reservation from cloud_jobs order by created_at")
            .fetch_all(&db.owner)
            .await
            .unwrap();
    assert_eq!(
        rows[0],
        (cancelled.id.clone(), "cancelled".into(), "released".into())
    );
    assert_eq!(
        rows[1],
        (open.id.clone(), "failed".into(), "released".into())
    );
    // A second pass rewrites nothing: no row still holds anything.
    let touched = sqlx::query("update cloud_jobs set updated_at=updated_at where expires_at<=now() and (reservation='open' or status not in ('succeeded','failed','cancelled'))")
        .execute(&db.owner)
        .await
        .unwrap()
        .rows_affected();
    assert_eq!(touched, 0);
    db.drop().await;
}

#[tokio::test]
async fn a_worker_shutdown_requeues_without_spending_an_attempt() {
    let Some(db) = test_db().await else { return };
    setup(&db.owner, "usr_a", "api", 100).await;
    key(&db.owner).await;
    let job = jobs::create(&db.server, &who("usr_a", true), &data(10))
        .await
        .unwrap();
    sqlx::query("update cloud_jobs set status='queued' where id=$1")
        .bind(&job.id)
        .execute(&db.owner)
        .await
        .unwrap();
    let first = jobs::claim(&db.server, "w1").await.unwrap().unwrap();
    assert!(jobs::requeue(&db.server, &first).await.unwrap());
    // The old attempt is fenced off.
    assert!(
        !jobs::finish(&db.server, &first, &["x".into()], None)
            .await
            .unwrap()
    );
    let second = jobs::claim(&db.server, "w2").await.unwrap().unwrap();
    assert_eq!(second.id, job.id);
    assert_eq!(second.attempt, 2);
    assert_eq!(second.max_attempts, 4);
    assert!(
        jobs::finish(&db.server, &second, &["x".into()], None)
            .await
            .unwrap()
    );
    db.drop().await;
}

#[test]
fn job_views_never_show_url_secrets() {
    assert_eq!(
        routes::redact_url("https://bucket.s3.amazonaws.com/a/photo.png?X-Amz-Signature=abc"),
        "https://bucket.s3.amazonaws.com/…"
    );
    assert_eq!(
        routes::redact_url("https://hooks.example.com/t0ken-in-path/convt"),
        "https://hooks.example.com/…"
    );
    assert_eq!(
        routes::redact_url("https://user:pw@example.com"),
        "https://example.com"
    );
}

#[tokio::test]
async fn web_credentials_cannot_read_api_jobs() {
    let Some(db) = test_db().await else { return };
    setup(&db.owner, "usr_a", "api", 100).await;
    key(&db.owner).await;
    let state = state_with(&db, Arc::new(MemoryStorage::default()));
    let (status, _, created) = send(
        &state,
        authed("POST", "/v1/jobs")
            .header("content-type", "application/json")
            .body(Body::from(
                r#"{"target_format":"webp","source_url":"https://cdn.example.com/a.png?sig=secret"}"#,
            ))
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let created = json(&created);
    assert_eq!(created["job"]["source_url"], "https://cdn.example.com/…");
    let id = created["job"]["id"].as_str().unwrap();
    let web = web_token(&state.web_secret, "usr_a");
    for (method, path) in [
        ("GET", format!("/v1/jobs/{id}")),
        ("POST", format!("/v1/jobs/{id}/cancel")),
    ] {
        let (status, _, _) = send(
            &state,
            Request::builder()
                .method(method)
                .uri(path)
                .header("authorization", format!("Bearer {web}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await;
        assert_eq!(status, StatusCode::NOT_FOUND);
    }
    db.drop().await;
}

#[tokio::test]
async fn keyed_replays_recover_jobs_even_with_a_feature_switched_off() {
    let Some(db) = test_db().await else { return };
    setup(&db.owner, "usr_a", "api", 100).await;
    key(&db.owner).await;
    let state = state_with(&db, Arc::new(MemoryStorage::default()));
    let body = r#"{"target_format":"webp","source_url":"https://cdn.example.com/a.png"}"#;
    let create = |state: &CloudState, key: &'static str| {
        let state = state.clone();
        async move {
            send(
                &state,
                authed("POST", "/v1/jobs")
                    .header("content-type", "application/json")
                    .header("idempotency-key", key)
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
        }
    };
    let (status, _, first) = create(&state, "remote-1").await;
    assert_eq!(status, StatusCode::OK);
    let mut off = state.clone();
    off.source_urls = false;
    let (status, headers, replay) = create(&off, "remote-1").await;
    assert_eq!(status, StatusCode::OK, "{}", json(&replay));
    assert_eq!(headers["idempotent-replayed"], "true");
    assert_eq!(json(&replay)["job"]["id"], json(&first)["job"]["id"]);
    let (status, _, refused) = create(&off, "remote-2").await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(json(&refused)["error"]["code"], "source_url_unavailable");
    db.drop().await;
}

#[tokio::test]
async fn upload_urls_never_outlive_their_job_and_expiry_drops_customer_urls() {
    let Some(db) = test_db().await else { return };
    setup(&db.owner, "usr_a", "api", 100).await;
    key(&db.owner).await;
    let state = state_with(&db, Arc::new(MemoryStorage::default()));
    let body = r#"{"input_format":"png","target_format":"webp","input_bytes":4,"webhook_url":"https://hooks.example.com/secret-path"}"#;
    let request = || {
        authed("POST", "/v1/jobs")
            .header("content-type", "application/json")
            .header("idempotency-key", "late-1")
            .body(Body::from(body))
            .unwrap()
    };
    let (_, _, first) = send(&state, request()).await;
    assert_eq!(json(&first)["upload_expires_in"], 900);
    let id = json(&first)["job"]["id"].as_str().unwrap().to_owned();
    sqlx::query("update cloud_jobs set expires_at=now()+interval '5 minutes' where id=$1")
        .bind(&id)
        .execute(&db.owner)
        .await
        .unwrap();
    let (_, _, replay) = send(&state, request()).await;
    let ttl = json(&replay)["upload_expires_in"].as_i64().unwrap();
    assert!((60..=240).contains(&ttl), "{ttl}");
    sqlx::query("update cloud_jobs set expires_at=now()+interval '90 seconds' where id=$1")
        .bind(&id)
        .execute(&db.owner)
        .await
        .unwrap();
    let (_, _, late) = send(&state, request()).await;
    assert!(json(&late)["upload_url"].is_null());
    // Expired with its webhook delivered: both customer URLs are cleared.
    sqlx::query("update cloud_jobs set expires_at=now()-interval '1 second', source_url='https://cdn.example.com/x?sig=1', webhook_delivered_at=now() where id=$1")
        .bind(&id)
        .execute(&db.owner)
        .await
        .unwrap();
    routes::expire(&state).await.unwrap();
    let (source, webhook): (Option<String>, Option<String>) =
        sqlx::query_as("select source_url, webhook_url from cloud_jobs where id=$1")
            .bind(&id)
            .fetch_one(&db.owner)
            .await
            .unwrap();
    assert_eq!((source, webhook), (None, None));
    db.drop().await;
}

#[tokio::test]
async fn a_keyed_upload_with_different_bytes_is_refused_and_metadata_is_capped() {
    let Some(db) = test_db().await else { return };
    setup(&db.owner, "usr_a", "api", 100).await;
    key(&db.owner).await;
    let state = state_with(&db, Arc::new(MemoryStorage::default()));
    let create = |hash: &str| {
        authed("POST", "/v1/jobs")
            .header("content-type", "application/json")
            .header("idempotency-key", "same-size")
            .body(Body::from(format!(
                r#"{{"input_format":"png","target_format":"webp","input_bytes":7,"input_sha256":"{hash}"}}"#
            )))
            .unwrap()
    };
    let (status, _, _) = send(&state, create(&"a".repeat(64))).await;
    assert_eq!(status, StatusCode::OK);
    let (status, _, conflict) = send(&state, create(&"b".repeat(64))).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(json(&conflict)["error"]["code"], "idempotency_key_reused");
    let (status, _, bad) = send(&state, create("not-a-hash")).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(json(&bad)["error"]["code"], "invalid_request");
    let boundary = "B";
    let form = format!(
        "--{boundary}\r\nContent-Disposition: form-data; name=\"options\"\r\n\r\n{}\r\n--{boundary}--\r\n",
        "x".repeat(17 * 1024)
    );
    let (status, _, refused) = send(
        &state,
        authed("POST", "/v1/convert?to=webp&from=png")
            .header(
                "content-type",
                format!("multipart/form-data; boundary={boundary}"),
            )
            .body(Body::from(form))
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(
        json(&refused)["error"]["message"]
            .as_str()
            .unwrap()
            .contains("16 KB")
    );
    db.drop().await;
}

#[tokio::test]
async fn errors_after_a_job_exists_name_it() {
    use axum::response::IntoResponse as _;
    let response = routes::ApiError::new(
        StatusCode::INTERNAL_SERVER_ERROR,
        "internal",
        "Request failed.",
    )
    .with_job_id("job_abc")
    .into_response();
    assert_eq!(response.headers()["convt-job-id"], "job_abc");
    assert_eq!(response.headers()["location"], "/v1/jobs/job_abc");
    let body = json(&to_bytes(response.into_body(), 10_000).await.unwrap());
    assert_eq!(body["job_id"], "job_abc");
    assert!(
        body["error"]["message"]
            .as_str()
            .unwrap()
            .contains("GET /v1/jobs/job_abc")
    );
}

#[tokio::test]
async fn a_replayed_conversion_redirects_to_storage_instead_of_streaming_again() {
    let Some(db) = test_db().await else { return };
    setup(&db.owner, "usr_a", "api", 100).await;
    key(&db.owner).await;
    let storage = Arc::new(MemoryStorage::default());
    let state = state_with(&db, storage.clone());
    let request = || {
        authed("POST", "/v1/convert?to=webp&from=png")
            .header("idempotency-key", "replay-1")
            .body(Body::from("png-bytes"))
            .unwrap()
    };
    let worker = {
        let pool = db.server.clone();
        let storage = storage.clone();
        tokio::spawn(async move { work_one(&pool, &storage, b"webp").await })
    };
    let (status, _, body) = send(&state, request()).await;
    worker.await.unwrap();
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, b"webp");
    let (status, headers, body) = send(&state, request()).await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    assert!(
        headers["location"]
            .to_str()
            .unwrap()
            .starts_with("https://download.test/")
    );
    assert_eq!(headers["idempotent-replayed"], "true");
    assert_eq!(json(&body)["job"]["status"], "succeeded");
    db.drop().await;
}

#[tokio::test]
async fn trailing_json_is_refused_and_a_missing_output_keeps_the_job() {
    let Some(db) = test_db().await else { return };
    setup(&db.owner, "usr_a", "api", 100).await;
    key(&db.owner).await;
    let storage = Arc::new(MemoryStorage::default());
    let state = state_with(&db, storage.clone());
    let (status, _, body) = send(
        &state,
        authed("POST", "/v1/convert?wait=0")
            .header("content-type", "application/json")
            .body(Body::from(
                r#"{"source_url":"https://cdn.example.com/a.png","to":"webp"}{"x":1}"#,
            ))
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(json(&body)["error"]["code"], "invalid_request");
    let queued: i64 = sqlx::query_scalar("select count(*) from cloud_jobs")
        .fetch_one(&db.owner)
        .await
        .unwrap();
    assert_eq!(queued, 0);
    // A worker that reports an output the bucket no longer has.
    let worker = {
        let pool = db.server.clone();
        tokio::spawn(async move {
            for _ in 0..100 {
                if let Some(job) = jobs::claim(&pool, "w").await.unwrap() {
                    let key = format!("{}/attempt-1/input.webp", job.id);
                    assert!(jobs::finish(&pool, &job, &[key], None).await.unwrap());
                    return;
                }
                tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            }
        })
    };
    let (status, headers, body) = send(
        &state,
        authed("POST", "/v1/convert?to=webp&from=png")
            .body(Body::from("png"))
            .unwrap(),
    )
    .await;
    worker.await.unwrap();
    assert_eq!(status, StatusCode::BAD_GATEWAY);
    assert_eq!(json(&body)["error"]["code"], "output_missing");
    assert_eq!(json(&body)["job"]["status"], "succeeded");
    assert!(headers.contains_key("convt-job-id"));
    // The JSON form refuses to hand out a dead link too.
    let (status, _, _) = send(
        &state,
        authed("POST", "/v1/convert?to=webp&from=png&wait=0")
            .header("accept", "application/json")
            .header("idempotency-key", "json-missing")
            .body(Body::from("png2"))
            .unwrap(),
    )
    .await;
    // No worker this time, so the job is still queued; finish it by hand with an
    // output that was never stored.
    assert_eq!(status, StatusCode::ACCEPTED);
    let job_id: String =
        sqlx::query_scalar("select id from cloud_jobs where idempotency_key='json-missing'")
            .fetch_one(&db.owner)
            .await
            .unwrap();
    sqlx::query("update cloud_jobs set status='succeeded', reservation='settled', output_keys=$2 where id=$1")
        .bind(&job_id)
        .bind(serde_json::json!([format!("{job_id}/attempt-1/gone.webp")]))
        .execute(&db.owner)
        .await
        .unwrap();
    let (status, _, body) = send(
        &state,
        authed("POST", "/v1/convert?to=webp&from=png")
            .header("accept", "application/json")
            .header("idempotency-key", "json-missing")
            .body(Body::from("png2"))
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_GATEWAY);
    assert_eq!(json(&body)["error"]["code"], "output_missing");
    db.drop().await;
}
