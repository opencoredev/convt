//! Finished conversions, kept in SQLite in the convt data directory so the
//! History tab survives restarts.

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use convt_core::{Options, Output};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};

/// How many rows the History tab keeps. Older ones are pruned on insert.
const KEEP: i64 = 1000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    Done(Vec<PathBuf>),
    Failed(String),
    Cancelled,
}

/// How a conversion was asked to run, so Retry can run it the same way.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Setup {
    pub options: Options,
    pub output: Output,
    /// It ran on convt's cloud, so Retry runs it there again.
    pub cloud: bool,
}

#[derive(Serialize, Deserialize)]
struct StoredSetup {
    options: Options,
    output: StoredOutput,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    cloud: bool,
}

/// [`Output`] with byte paths, so non-UTF-8 names survive.
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
enum StoredOutput {
    Beside,
    Dir(Vec<u8>),
    Exact(Vec<u8>),
}

impl Setup {
    fn to_json(&self) -> String {
        let output = match &self.output {
            Output::Beside => StoredOutput::Beside,
            Output::Dir(dir) => StoredOutput::Dir(path_bytes(dir)),
            Output::Exact(path) => StoredOutput::Exact(path_bytes(path)),
        };
        let stored = StoredSetup {
            options: self.options.clone(),
            output,
            cloud: self.cloud,
        };
        serde_json::to_string(&stored).expect("options serialize")
    }

    fn from_json(text: &str) -> Option<Self> {
        let stored: StoredSetup = serde_json::from_str(text).ok()?;
        let output = match stored.output {
            StoredOutput::Beside => Output::Beside,
            StoredOutput::Dir(dir) => Output::Dir(path_from_bytes(dir)),
            StoredOutput::Exact(path) => Output::Exact(path_from_bytes(path)),
        };
        Some(Self {
            options: stored.options,
            output,
            cloud: stored.cloud,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Record {
    pub id: i64,
    /// Unix seconds.
    pub finished_at: i64,
    pub input: PathBuf,
    /// Target format id.
    pub to: String,
    pub outcome: Outcome,
    /// How it ran; `None` for records from before the app kept it.
    pub setup: Option<Setup>,
}

pub struct History {
    db: Connection,
}

impl History {
    pub fn path() -> Option<PathBuf> {
        convt_engines::paths::data_dir().map(|d| d.join("history.sqlite3"))
    }

    pub fn open(path: &Path) -> rusqlite::Result<Self> {
        if let Some(dir) = path.parent() {
            // Private, whatever the umask: the document pack refuses to
            // install under a folder other users can write to.
            let mut builder = std::fs::DirBuilder::new();
            builder.recursive(true);
            #[cfg(unix)]
            std::os::unix::fs::DirBuilderExt::mode(&mut builder, 0o700);
            let _ = builder.create(dir);
        }
        Self::init(Connection::open(path)?)
    }

    pub fn in_memory() -> rusqlite::Result<Self> {
        Self::init(Connection::open_in_memory()?)
    }

    fn init(db: Connection) -> rusqlite::Result<Self> {
        db.execute_batch(
            "PRAGMA journal_mode = WAL;
             CREATE TABLE IF NOT EXISTS history (
                 id INTEGER PRIMARY KEY,
                 finished_at INTEGER NOT NULL,
                 input BLOB NOT NULL,
                 target TEXT NOT NULL,
                 status TEXT NOT NULL,
                 outputs TEXT NOT NULL DEFAULT '[]',
                 error TEXT
             );",
        )?;
        let has_setup = db
            .query_row(
                "SELECT 1 FROM pragma_table_info('history') WHERE name = 'setup'",
                [],
                |_| Ok(()),
            )
            .optional()?
            .is_some();
        if !has_setup {
            db.execute("ALTER TABLE history ADD COLUMN setup TEXT", [])?;
        }
        Ok(Self { db })
    }

    pub fn add(
        &self,
        input: &Path,
        to: &str,
        setup: &Setup,
        outcome: &Outcome,
    ) -> rusqlite::Result<i64> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_secs() as i64);
        let (status, outputs, error) = match outcome {
            Outcome::Done(paths) => ("done", paths_to_json(paths), None),
            Outcome::Failed(e) => ("failed", "[]".into(), Some(e.as_str())),
            Outcome::Cancelled => ("cancelled", "[]".into(), None),
        };
        self.db.execute(
            "INSERT INTO history (finished_at, input, target, status, outputs, error, setup)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                now,
                path_bytes(input),
                to,
                status,
                outputs,
                error,
                setup.to_json()
            ],
        )?;
        let id = self.db.last_insert_rowid();
        self.db
            .execute("DELETE FROM history WHERE id <= ?1", params![id - KEEP])?;
        Ok(id)
    }

    /// The newest `limit` records, newest first.
    pub fn recent(&self, limit: usize) -> rusqlite::Result<Vec<Record>> {
        let mut stmt = self.db.prepare(
            "SELECT id, finished_at, input, target, status, outputs, error, setup
             FROM history ORDER BY id DESC LIMIT ?1",
        )?;
        let rows = stmt.query_map(params![limit as i64], |row| {
            let status: String = row.get(4)?;
            let outputs: String = row.get(5)?;
            let error: Option<String> = row.get(6)?;
            let setup: Option<String> = row.get(7)?;
            let outcome = match status.as_str() {
                "done" => Outcome::Done(paths_from_json(&outputs)),
                "cancelled" => Outcome::Cancelled,
                _ => Outcome::Failed(error.unwrap_or_default()),
            };
            Ok(Record {
                id: row.get(0)?,
                finished_at: row.get(1)?,
                input: path_from_bytes(row.get(2)?),
                to: row.get(3)?,
                outcome,
                setup: setup.as_deref().and_then(Setup::from_json),
            })
        })?;
        rows.collect()
    }

    pub fn clear(&self) -> rusqlite::Result<()> {
        self.db.execute("DELETE FROM history", []).map(drop)
    }
}

#[cfg(unix)]
fn path_bytes(p: &Path) -> Vec<u8> {
    use std::os::unix::ffi::OsStrExt;
    p.as_os_str().as_bytes().to_vec()
}

#[cfg(not(unix))]
fn path_bytes(p: &Path) -> Vec<u8> {
    p.to_string_lossy().into_owned().into_bytes()
}

#[cfg(unix)]
fn path_from_bytes(b: Vec<u8>) -> PathBuf {
    use std::os::unix::ffi::OsStringExt;
    std::ffi::OsString::from_vec(b).into()
}

#[cfg(not(unix))]
fn path_from_bytes(b: Vec<u8>) -> PathBuf {
    String::from_utf8_lossy(&b).into_owned().into()
}

/// Output paths as a JSON array of byte arrays, so non-UTF-8 names survive.
fn paths_to_json(paths: &[PathBuf]) -> String {
    let raw: Vec<Vec<u8>> = paths.iter().map(|p| path_bytes(p)).collect();
    serde_json::to_string(&raw).expect("byte arrays serialize")
}

fn paths_from_json(text: &str) -> Vec<PathBuf> {
    serde_json::from_str::<Vec<Vec<u8>>>(text)
        .unwrap_or_default()
        .into_iter()
        .map(path_from_bytes)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn records_round_trip_newest_first() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("data/history.sqlite3");
        {
            let h = History::open(&path).unwrap();
            let out = vec![PathBuf::from("/out/a b.jpg"), PathBuf::from("/out/a-2.jpg")];
            h.add(
                Path::new("/in/a b.png"),
                "jpeg",
                &Setup::default(),
                &Outcome::Done(out),
            )
            .unwrap();
            h.add(
                Path::new("/in/c.pdf"),
                "png",
                &Setup::default(),
                &Outcome::Failed("no engine".into()),
            )
            .unwrap();
            h.add(
                Path::new("/in/d.mp4"),
                "webm",
                &Setup::default(),
                &Outcome::Cancelled,
            )
            .unwrap();
        }
        // The data folder is private, so the document pack may go in it.
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(dir.path().join("data"))
                .unwrap()
                .permissions()
                .mode();
            assert_eq!(mode & 0o777, 0o700);
        }
        let h = History::open(&path).unwrap();
        let recs = h.recent(10).unwrap();
        assert_eq!(recs.len(), 3);
        assert_eq!(recs[0].outcome, Outcome::Cancelled);
        assert_eq!(recs[1].outcome, Outcome::Failed("no engine".into()));
        assert_eq!(recs[2].input, PathBuf::from("/in/a b.png"));
        assert!(matches!(&recs[2].outcome, Outcome::Done(p) if p.len() == 2));
        assert_eq!(h.recent(1).unwrap().len(), 1);
        h.clear().unwrap();
        assert!(h.recent(10).unwrap().is_empty());
    }

    #[test]
    fn prunes_old_rows() {
        let h = History::in_memory().unwrap();
        for _ in 0..KEEP + 5 {
            h.add(
                Path::new("/a"),
                "png",
                &Setup::default(),
                &Outcome::Cancelled,
            )
            .unwrap();
        }
        assert_eq!(h.recent(usize::MAX >> 1).unwrap().len(), KEEP as usize);
    }

    #[test]
    fn keeps_how_a_conversion_ran() {
        let h = History::in_memory().unwrap();
        let setup = Setup {
            options: Options {
                quality: Some(40),
                video_height: Some(480),
                strip_audio: true,
                ..Options::default()
            },
            output: Output::Dir(PathBuf::from("/out/converted")),
            cloud: false,
        };
        h.add(Path::new("/in/a.mov"), "mkv", &setup, &Outcome::Cancelled)
            .unwrap();
        assert_eq!(h.recent(1).unwrap()[0].setup, Some(setup.clone()));
        // A cloud conversion is kept as one, so Retry runs it there again.
        let cloud = Setup {
            cloud: true,
            ..setup
        };
        h.add(Path::new("/in/a.png"), "webp", &cloud, &Outcome::Cancelled)
            .unwrap();
        assert_eq!(h.recent(1).unwrap()[0].setup, Some(cloud));
    }

    #[test]
    fn opens_a_history_from_before_setups_were_kept() {
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch(
            "CREATE TABLE history (
                 id INTEGER PRIMARY KEY,
                 finished_at INTEGER NOT NULL,
                 input BLOB NOT NULL,
                 target TEXT NOT NULL,
                 status TEXT NOT NULL,
                 outputs TEXT NOT NULL DEFAULT '[]',
                 error TEXT
             );
             INSERT INTO history (finished_at, input, target, status)
             VALUES (1, X'2F612E706E67', 'webp', 'cancelled');",
        )
        .unwrap();
        let h = History::init(db).unwrap();
        let old = &h.recent(10).unwrap()[0];
        assert_eq!(old.input, PathBuf::from("/a.png"));
        assert_eq!(old.setup, None);
    }
}
