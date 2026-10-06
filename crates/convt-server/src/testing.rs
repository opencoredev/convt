//! Databases for tests. `packages/db/scripts/test-db.sh` starts a disposable
//! Postgres with a migrated template and sets CONVT_TEST_DATABASE_URL (a superuser
//! URL). Each test copies the template into its own database and connects with
//! `SET ROLE convt_server`, so the role's grants apply. Without the variable the
//! tests skip; with CONVT_REQUIRE_DB=1 a skip fails.

use std::str::FromStr;

use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
use sqlx::{AssertSqlSafe, Executor, PgPool};

pub struct TestDb {
    pub name: String,
    /// Connected as convt_server.
    pub server: PgPool,
    /// Connected as convt_owner, for setting up rows the server may not write.
    pub owner: PgPool,
    admin: PgConnectOptions,
}

async fn pool_as(options: &PgConnectOptions, role: &'static str) -> PgPool {
    PgPoolOptions::new()
        .max_connections(4)
        .after_connect(move |conn, _| {
            Box::pin(async move {
                // Names here are fixed role and database names, not input.
                conn.execute(AssertSqlSafe(format!("set role {role}")))
                    .await?;
                Ok(())
            })
        })
        .connect_with(options.clone())
        .await
        .expect("connect to the test database")
}

/// A fresh database, or None when no test database is configured.
pub async fn test_db() -> Option<TestDb> {
    let Ok(url) = std::env::var("CONVT_TEST_DATABASE_URL") else {
        assert!(
            std::env::var("CONVT_REQUIRE_DB").as_deref() != Ok("1"),
            "CONVT_REQUIRE_DB=1 but CONVT_TEST_DATABASE_URL is not set"
        );
        eprintln!("skipping: CONVT_TEST_DATABASE_URL is not set");
        return None;
    };
    let template = std::env::var("TEST_TEMPLATE_DB").unwrap_or_else(|_| "convt_template".into());
    let admin = PgConnectOptions::from_str(&url).expect("CONVT_TEST_DATABASE_URL is a URL");
    let name = format!("r_{}", crate::ids::new_id("t").replace('_', ""));
    let admin_pool = PgPoolOptions::new()
        .max_connections(1)
        .connect_with(admin.clone())
        .await
        .expect("connect as the test superuser");
    admin_pool
        .execute(AssertSqlSafe(format!(
            "create database {name} template {template} owner convt_owner"
        )))
        .await
        .expect("create the test database");
    admin_pool.close().await;
    let options = admin.clone().database(&name);
    Some(TestDb {
        server: pool_as(&options, "convt_server").await,
        owner: pool_as(&options, "convt_owner").await,
        name,
        admin,
    })
}

impl TestDb {
    pub async fn drop(self) {
        self.server.close().await;
        self.owner.close().await;
        let admin = PgPoolOptions::new()
            .max_connections(1)
            .connect_with(self.admin)
            .await
            .expect("connect as the test superuser");
        admin
            .execute(AssertSqlSafe(format!(
                "drop database if exists {} with (force)",
                self.name
            )))
            .await
            .expect("drop the test database");
        admin.close().await;
    }
}
