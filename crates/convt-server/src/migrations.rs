//! The startup migration check. Drizzle's migrator records each applied migration
//! as (SHA-256 of the SQL file, journal `when`) and never compares hashes, so the
//! server does: it refuses to serve unless every migration it was built against is
//! applied, unchanged and in order. Newer rows are allowed with a warning, because
//! migrations deploy before the code that needs them and must be additive.

use sqlx::PgPool;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Migration {
    pub when: i64,
    pub tag: &'static str,
    pub hash: &'static str,
}

include!(concat!(env!("OUT_DIR"), "/migrations.rs"));

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Applied {
    pub when: i64,
    pub hash: String,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum MigrationError {
    #[error("migration {0} is not applied")]
    Missing(&'static str),
    #[error("migration {0} was applied from a different file")]
    Edited(&'static str),
    #[error("migration {expected} is applied out of order (found {found} in its place)")]
    OutOfOrder { expected: &'static str, found: i64 },
}

/// Compares what the build embeds with what the database recorded. Returns how
/// many newer migrations the database has.
pub fn compare(embedded: &[Migration], applied: &[Applied]) -> Result<usize, MigrationError> {
    for (i, m) in embedded.iter().enumerate() {
        let Some(row) = applied.get(i) else {
            return Err(MigrationError::Missing(m.tag));
        };
        if row.when != m.when {
            return if applied.iter().any(|a| a.when == m.when) {
                Err(MigrationError::OutOfOrder {
                    expected: m.tag,
                    found: row.when,
                })
            } else {
                Err(MigrationError::Missing(m.tag))
            };
        }
        if row.hash != m.hash {
            return Err(MigrationError::Edited(m.tag));
        }
    }
    Ok(applied.len().saturating_sub(embedded.len()))
}

pub async fn applied(pool: &PgPool) -> sqlx::Result<Vec<Applied>> {
    let exists = sqlx::query_scalar!(
        r#"select to_regclass('drizzle.__drizzle_migrations') is not null as "exists!""#
    )
    .fetch_one(pool)
    .await?;
    if !exists {
        return Ok(Vec::new());
    }
    let rows = sqlx::query!(
        r#"select hash, created_at as "created_at!" from drizzle.__drizzle_migrations order by created_at, id"#
    )
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| Applied {
            when: r.created_at,
            hash: r.hash,
        })
        .collect())
}

/// Refuses to start on a database that lacks or changed a migration this build needs.
pub async fn check(pool: &PgPool) -> anyhow::Result<()> {
    let applied = applied(pool).await?;
    match compare(EMBEDDED, &applied) {
        Ok(0) => {
            tracing::info!(
                migrations = EMBEDDED.len(),
                "database schema matches this build"
            );
            Ok(())
        }
        Ok(newer) => {
            tracing::warn!(newer, "the database has migrations newer than this build");
            Ok(())
        }
        Err(e) => Err(anyhow::anyhow!("refusing to serve: {e}")),
    }
}
