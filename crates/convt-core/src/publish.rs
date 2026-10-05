//! Moves finished artifacts from a job's staging directory to their final
//! names. Each name is reserved with an exclusive create before the move, so
//! a conversion never replaces an existing file, including its own input or
//! the output of another job that picked the same name.

use std::fs::OpenOptions;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use crate::{Error, Result};

/// How many ` (n)` suffixes to try before giving up.
const MAX_SUFFIX: u32 = 9999;

/// `photo.png`, `photo-2.png` for page 2, `photo (1).png` on collision. An
/// empty `ext` gives a name without one.
fn name(stem: &str, ext: &str, page: usize, dup: u32) -> String {
    let page = if page == 0 {
        String::new()
    } else {
        format!("-{}", page + 1)
    };
    let dup = if dup == 0 {
        String::new()
    } else {
        format!(" ({dup})")
    };
    if ext.is_empty() {
        format!("{stem}{page}{dup}")
    } else {
        format!("{stem}{page}{dup}.{ext}")
    }
}

/// Reserves every name in `paths`, or none of them.
fn reserve_all(paths: &[PathBuf]) -> std::io::Result<bool> {
    let mut reserved = Vec::new();
    for path in paths {
        match OpenOptions::new().write(true).create_new(true).open(path) {
            Ok(_) => reserved.push(path),
            Err(e) => {
                for p in reserved {
                    let _ = std::fs::remove_file(p);
                }
                return if e.kind() == ErrorKind::AlreadyExists {
                    Ok(false)
                } else {
                    Err(e)
                };
            }
        }
    }
    Ok(true)
}

/// Publishes `artifacts` into `dir` as `stem.ext`, `stem-2.ext`, ..., where
/// `pages` holds each artifact's 0-based page number. When `exact` is set, a
/// taken name is an error instead of getting a suffix.
pub(crate) fn publish(
    artifacts: &[PathBuf],
    pages: &[usize],
    dir: &Path,
    stem: &str,
    ext: &str,
    exact: bool,
) -> Result<Vec<PathBuf>> {
    let names = |dup| -> Vec<PathBuf> {
        pages
            .iter()
            .map(|&page| dir.join(name(stem, ext, page, dup)))
            .collect()
    };
    let mut targets = None;
    for dup in 0..=MAX_SUFFIX {
        let candidate = names(dup);
        if reserve_all(&candidate)? {
            targets = Some(candidate);
            break;
        }
        if exact {
            let taken = candidate.into_iter().find(|p| p.exists());
            return Err(Error::OutputExists(taken.unwrap_or_else(|| dir.join(stem))));
        }
    }
    let targets = targets.ok_or_else(|| Error::OutputExists(dir.join(name(stem, ext, 0, 0))))?;
    for (from, to) in artifacts.iter().zip(&targets) {
        // The staging directory sits inside `dir`, so this is a same-volume
        // rename that replaces the empty placeholder.
        if let Err(e) = std::fs::rename(from, to) {
            for t in &targets {
                let _ = std::fs::remove_file(t);
            }
            return Err(e.into());
        }
    }
    Ok(targets)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn staged(dir: &Path, n: usize) -> Vec<PathBuf> {
        (0..n)
            .map(|i| {
                let p = dir.join(format!("{i}.tmp"));
                std::fs::write(&p, format!("page {i}")).unwrap();
                p
            })
            .collect()
    }

    #[test]
    fn names_pages_and_collisions() {
        assert_eq!(name("a", "png", 0, 0), "a.png");
        assert_eq!(name("a", "png", 1, 0), "a-2.png");
        assert_eq!(name("a", "png", 2, 3), "a-3 (3).png");
    }

    #[test]
    fn never_replaces_existing_files() {
        let dir = tempfile::tempdir().unwrap();
        let d = dir.path();
        std::fs::write(d.join("x.png"), "keep").unwrap();
        std::fs::write(d.join("x-2 (1).png"), "keep").unwrap();
        let stage = tempfile::tempdir_in(d).unwrap();
        let out = publish(&staged(stage.path(), 2), &[0, 1], d, "x", "png", false).unwrap();
        // x.png is taken, and x-2 (1).png blocks suffix 1 for the whole set.
        assert_eq!(out, [d.join("x (2).png"), d.join("x-2 (2).png")]);
        assert_eq!(std::fs::read_to_string(d.join("x.png")).unwrap(), "keep");
        assert_eq!(std::fs::read_to_string(&out[1]).unwrap(), "page 1");
        // No placeholder from the failed attempts is left behind.
        assert!(!d.join("x-2.png").exists() && !d.join("x (1).png").exists());
    }

    #[test]
    fn exact_refuses_taken_name() {
        let dir = tempfile::tempdir().unwrap();
        let d = dir.path();
        std::fs::write(d.join("x.png"), "keep").unwrap();
        let stage = tempfile::tempdir_in(d).unwrap();
        let err = publish(&staged(stage.path(), 1), &[0], d, "x", "png", true).unwrap_err();
        assert!(matches!(err, Error::OutputExists(p) if p == d.join("x.png")));
        assert_eq!(std::fs::read_to_string(d.join("x.png")).unwrap(), "keep");
    }
}
