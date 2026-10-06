//! API key lookup. Keys are `cvt_live_` plus 32 base32 characters; the database
//! keeps only the SHA-256 of the whole key (see `packages/license/src/api-key.ts`).
//! P9 adds the routes that use this; P6 provides the lookup and its tests.

use sha2::{Digest, Sha256};
use sqlx::PgPool;

pub const SCHEME: &str = "cvt_live_";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyOwner {
    pub key_id: String,
    pub user_id: String,
}

pub fn hash_key(key: &str) -> [u8; 32] {
    Sha256::digest(key.as_bytes()).into()
}

/// The owner of an unrevoked key, or None. Malformed keys never reach the database.
pub async fn find_by_key(pool: &PgPool, key: &str) -> sqlx::Result<Option<KeyOwner>> {
    let well_formed = key.len() == SCHEME.len() + 32
        && key.starts_with(SCHEME)
        && key[SCHEME.len()..]
            .bytes()
            .all(|b| b.is_ascii_digit() || b.is_ascii_lowercase());
    if !well_formed {
        return Ok(None);
    }
    let hash = hash_key(key);
    let row = sqlx::query!(
        "select id, user_id from api_keys where secret_hash = $1 and revoked_at is null",
        &hash[..]
    )
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|r| KeyOwner {
        key_id: r.id,
        user_id: r.user_id,
    }))
}

/// Records that a key was used. Only `last_used_at` is writable for convt_server.
pub async fn touch(pool: &PgPool, key_id: &str) -> sqlx::Result<()> {
    sqlx::query!(
        "update api_keys set last_used_at = now() where id = $1",
        key_id
    )
    .execute(pool)
    .await?;
    Ok(())
}
