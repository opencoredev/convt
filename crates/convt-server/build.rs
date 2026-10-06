//! Embeds the database migrations convt-server was built against: for each entry
//! in `packages/db/migrations/meta/_journal.json`, its `when` timestamp, tag and the
//! SHA-256 of its SQL file, the same values Drizzle's migrator records in
//! `drizzle.__drizzle_migrations`. At startup the server refuses to serve unless the
//! database has every one of them, unchanged and in order (see `src/migrations.rs`).

use std::fmt::Write as _;
use std::path::Path;

use serde::Deserialize;
use sha2::{Digest, Sha256};

#[derive(Deserialize)]
struct Journal {
    entries: Vec<Entry>,
}

#[derive(Deserialize)]
struct Entry {
    when: i64,
    tag: String,
}

fn main() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../packages/db/migrations");
    let journal_path = dir.join("meta/_journal.json");
    println!("cargo:rerun-if-changed={}", dir.display());
    println!("cargo:rerun-if-changed={}", journal_path.display());
    let journal: Journal = serde_json::from_str(
        &std::fs::read_to_string(&journal_path)
            .unwrap_or_else(|e| panic!("reading {}: {e}", journal_path.display())),
    )
    .expect("the migration journal is valid JSON");

    let mut out = String::from("pub const EMBEDDED: &[Migration] = &[\n");
    for entry in &journal.entries {
        let sql_path = dir.join(format!("{}.sql", entry.tag));
        println!("cargo:rerun-if-changed={}", sql_path.display());
        let sql = std::fs::read(&sql_path)
            .unwrap_or_else(|e| panic!("reading {}: {e}", sql_path.display()));
        let hash = hex::encode(Sha256::digest(&sql));
        writeln!(
            out,
            "    Migration {{ when: {}, tag: {:?}, hash: {:?} }},",
            entry.when, entry.tag, hash
        )
        .unwrap();
    }
    out.push_str("];\n");
    let dest = Path::new(&std::env::var("OUT_DIR").unwrap()).join("migrations.rs");
    std::fs::write(dest, out).unwrap();
}
