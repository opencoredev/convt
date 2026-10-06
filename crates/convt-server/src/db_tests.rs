//! Database tests. Run them through `packages/db/scripts/test-db.sh cargo test -p
//! convt-server`; without CONVT_TEST_DATABASE_URL they skip.

use chrono::{Duration, Utc};
use sqlx::PgPool;

use crate::testing::test_db;
use crate::{api_keys, cleanup, migrations, usage};

const KEY: &str = "cvt_live_0123456789abcdefghjkmnpqrstvwxyz";

async fn add_user(owner: &PgPool, id: &str) {
    sqlx::query("insert into users (id, name, email, email_verified) values ($1, '', $2, true)")
        .bind(id)
        .bind(format!("{id}@convt.test"))
        .execute(owner)
        .await
        .unwrap();
}

async fn add_key(owner: &PgPool, id: &str, user: &str, key: &str) {
    sqlx::query("insert into api_keys (id, user_id, name, prefix, secret_hash) values ($1, $2, 'k', $3, $4)")
        .bind(id)
        .bind(user)
        .bind(&key[..17])
        .bind(&api_keys::hash_key(key)[..])
        .execute(owner)
        .await
        .unwrap();
}

#[test]
fn key_hash_matches_the_shared_vector() {
    let v: serde_json::Value =
        serde_json::from_str(include_str!("../../../packages/license/vectors/ids.json")).unwrap();
    let key = v["api_key"]["key"].as_str().unwrap();
    assert_eq!(
        hex::encode(api_keys::hash_key(key)),
        v["api_key"]["sha256_hex"]
    );
    assert_eq!(key, KEY);
}

#[tokio::test]
async fn api_key_lookup_by_hash() {
    let Some(db) = test_db().await else { return };
    add_user(&db.owner, "usr_a").await;
    add_key(&db.owner, "key_a", "usr_a", KEY).await;
    let found = api_keys::find_by_key(&db.server, KEY).await.unwrap();
    assert_eq!(
        found,
        Some(api_keys::KeyOwner {
            key_id: "key_a".into(),
            user_id: "usr_a".into()
        })
    );
    let other = KEY.replace("0123", "0124");
    assert_eq!(
        api_keys::find_by_key(&db.server, &other).await.unwrap(),
        None
    );
    for malformed in [
        "",
        "cvt_live_",
        "cvt_test_0123456789abcdefghjkmnpqrstvwx",
        &KEY.to_uppercase(),
    ] {
        assert_eq!(
            api_keys::find_by_key(&db.server, malformed).await.unwrap(),
            None
        );
    }
    api_keys::touch(&db.server, "key_a").await.unwrap();
    let used: Option<chrono::DateTime<Utc>> =
        sqlx::query_scalar("select last_used_at from api_keys where id = 'key_a'")
            .fetch_one(&db.owner)
            .await
            .unwrap();
    assert!(used.is_some());
    db.drop().await;
}

#[tokio::test]
async fn revoked_keys_are_rejected() {
    let Some(db) = test_db().await else { return };
    add_user(&db.owner, "usr_a").await;
    add_key(&db.owner, "key_a", "usr_a", KEY).await;
    sqlx::query("update api_keys set revoked_at = now() where id = 'key_a'")
        .execute(&db.owner)
        .await
        .unwrap();
    assert_eq!(api_keys::find_by_key(&db.server, KEY).await.unwrap(), None);
    // convt_server can only touch last_used_at: it cannot un-revoke a key.
    let err = sqlx::query("update api_keys set revoked_at = null where id = 'key_a'")
        .execute(&db.server)
        .await
        .unwrap_err();
    assert!(err.to_string().contains("permission denied"), "{err}");
    db.drop().await;
}

#[tokio::test]
async fn usage_is_recorded_once_per_job() {
    let Some(db) = test_db().await else { return };
    add_user(&db.owner, "usr_a").await;
    let event = usage::NewUsage {
        user_id: "usr_a",
        subscription_id: None,
        api_key_id: None,
        job_id: "job_1",
        kind: "api_conversion",
        quantity: 1,
        amount_cents: 5,
        occurred_at: Utc::now(),
    };
    assert!(usage::record(&db.server, &event).await.unwrap());
    assert!(!usage::record(&db.server, &event).await.unwrap());
    let other_kind = usage::NewUsage {
        kind: "pro_bytes",
        ..event.clone()
    };
    assert!(usage::record(&db.server, &other_kind).await.unwrap());
    let n: i64 = sqlx::query_scalar("select count(*) from usage_events where job_id = 'job_1'")
        .fetch_one(&db.owner)
        .await
        .unwrap();
    assert_eq!(n, 2);
    // The rows are facts: convt_server cannot change the quantity.
    let err = sqlx::query("update usage_events set quantity = 100")
        .execute(&db.server)
        .await
        .unwrap_err();
    assert!(err.to_string().contains("permission denied"), "{err}");
    db.drop().await;
}

#[test]
fn compare_migrations() {
    use migrations::{Applied, Migration, MigrationError, compare};
    let embedded = [
        Migration {
            when: 1,
            tag: "0000_a",
            hash: "aa",
        },
        Migration {
            when: 2,
            tag: "0001_b",
            hash: "bb",
        },
    ];
    let row = |when, hash: &str| Applied {
        when,
        hash: hash.into(),
    };
    assert_eq!(compare(&embedded, &[row(1, "aa"), row(2, "bb")]), Ok(0));
    assert_eq!(
        compare(&embedded, &[row(1, "aa"), row(2, "bb"), row(3, "cc")]),
        Ok(1)
    );
    assert_eq!(
        compare(&embedded, &[row(1, "aa")]),
        Err(MigrationError::Missing("0001_b"))
    );
    assert_eq!(
        compare(&embedded, &[]),
        Err(MigrationError::Missing("0000_a"))
    );
    assert_eq!(
        compare(&embedded, &[row(1, "aa"), row(2, "xx")]),
        Err(MigrationError::Edited("0001_b"))
    );
    assert_eq!(
        compare(&embedded, &[row(2, "bb"), row(1, "aa")]),
        Err(MigrationError::OutOfOrder {
            expected: "0000_a",
            found: 2
        })
    );
}

#[tokio::test]
async fn startup_migration_check() {
    let Some(db) = test_db().await else { return };
    // The template was migrated from the same files this build embeds.
    migrations::check(&db.server)
        .await
        .expect("a fresh database passes");
    assert_eq!(
        migrations::applied(&db.server).await.unwrap().len(),
        migrations::EMBEDDED.len()
    );

    // A newer migration than the build knows is allowed.
    sqlx::query("insert into drizzle.__drizzle_migrations (hash, created_at) values ('newer', 99999999999999)")
        .execute(&db.owner)
        .await
        .unwrap();
    migrations::check(&db.server)
        .await
        .expect("a newer migration is allowed");

    // An edited migration is refused.
    let first = migrations::EMBEDDED[0];
    sqlx::query("update drizzle.__drizzle_migrations set hash = 'edited' where created_at = $1")
        .bind(first.when)
        .execute(&db.owner)
        .await
        .unwrap();
    let err = migrations::check(&db.server).await.unwrap_err().to_string();
    assert!(err.contains("different file"), "{err}");

    // A missing migration is refused.
    sqlx::query("delete from drizzle.__drizzle_migrations where created_at = $1")
        .bind(first.when)
        .execute(&db.owner)
        .await
        .unwrap();
    let err = migrations::check(&db.server).await.unwrap_err().to_string();
    assert!(
        err.contains("not applied") || err.contains("out of order"),
        "{err}"
    );
    db.drop().await;
}

#[tokio::test]
async fn cleanup_removes_only_expired_rows() {
    let Some(db) = test_db().await else { return };
    let now = Utc::now();
    for (id, expires) in [
        ("v_old", now - Duration::minutes(1)),
        ("v_new", now + Duration::minutes(10)),
        ("otp-issued:v_old", now - Duration::minutes(1)),
    ] {
        sqlx::query("insert into verifications (id, identifier, value, expires_at) values ($1, $1, 'x', $2)")
            .bind(id)
            .bind(expires)
            .execute(&db.owner)
            .await
            .unwrap();
        sqlx::query("insert into otp_send_limits (key, window_start, count, expires_at) values ($1, $2, 1, $3)")
            .bind(id)
            .bind(now)
            .bind(expires)
            .execute(&db.owner)
            .await
            .unwrap();
    }
    for (id, last) in [
        ("r_old", now.timestamp_millis() - 2 * 86_400_000),
        ("r_new", now.timestamp_millis()),
    ] {
        sqlx::query(
            "insert into rate_limits (id, key, count, last_request) values ($1, $1, 1, $2)",
        )
        .bind(id)
        .bind(last)
        .execute(&db.owner)
        .await
        .unwrap();
    }
    let removed = cleanup::run_once(&db.server, now).await.unwrap();
    assert_eq!(
        removed,
        cleanup::Removed {
            verifications: 2,
            rate_limits: 1,
            otp_send_limits: 2
        }
    );
    let left: Vec<String> = sqlx::query_scalar(
        "select id from verifications union all select key from otp_send_limits union all select id from rate_limits order by 1",
    )
    .fetch_all(&db.owner)
    .await
    .unwrap();
    assert_eq!(left, ["r_new", "v_new", "v_new"]);
    db.drop().await;
}
