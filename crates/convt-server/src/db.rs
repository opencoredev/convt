//! The Postgres pool. convt-server connects as `convt_server` (DATABASE_URL); it
//! never runs migrations, which `bun run db:migrate` applies as `convt_owner`.

use std::time::Duration;

use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;

pub async fn connect(url: &str) -> sqlx::Result<PgPool> {
    PgPoolOptions::new()
        .max_connections(10)
        .acquire_timeout(Duration::from_secs(10))
        .connect(url)
        .await
}
