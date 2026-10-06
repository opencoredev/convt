use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

use crate::{Cancel, Category, Error, Job, Registry, Result, format_by_extension};

/// A file found by [`expand_inputs`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BatchItem {
    pub input: PathBuf,
    /// Named directly rather than found in a folder. Callers report errors for
    /// explicit files and quietly skip folder files that can't convert.
    pub explicit: bool,
    /// The folder argument this file was found under, so callers can mirror
    /// subfolders in an output directory. `None` for explicit files.
    pub root: Option<PathBuf>,
}

impl BatchItem {
    /// The file's folder relative to the folder it was found under: empty for
    /// explicit files and files at the top of a folder argument.
    pub fn relative_dir(&self) -> PathBuf {
        self.root
            .as_ref()
            .and_then(|root| self.input.parent()?.strip_prefix(root).ok())
            .map(Path::to_path_buf)
            .unwrap_or_default()
    }
}

/// Expands folders into the files they contain, sorted by path. Hidden files
/// and folders are skipped; subfolders only with `recursive`.
pub fn expand_inputs(paths: &[PathBuf], recursive: bool) -> std::io::Result<Vec<BatchItem>> {
    let mut out = Vec::new();
    for path in paths {
        if path.is_dir() {
            let mut found = Vec::new();
            walk(path, recursive, &mut found)?;
            found.sort();
            out.extend(found.into_iter().map(|input| BatchItem {
                input,
                explicit: false,
                root: Some(path.clone()),
            }));
        } else {
            out.push(BatchItem {
                input: path.clone(),
                explicit: true,
                root: None,
            });
        }
    }
    Ok(out)
}

fn walk(dir: &Path, recursive: bool, out: &mut Vec<PathBuf>) -> std::io::Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        if entry.file_name().to_string_lossy().starts_with('.') {
            continue;
        }
        let kind = entry.file_type()?;
        let path = entry.path();
        // Symlinked folders are not followed, so a link loop can't recurse forever.
        if kind.is_dir() {
            if recursive {
                walk(&path, recursive, out)?;
            }
        } else if path.is_file() {
            out.push(path);
        }
    }
    Ok(())
}

/// Progress of one job in a batch.
#[derive(Debug)]
pub enum Event<'a> {
    Started,
    Progress(Option<f32>),
    Finished(&'a Result<Vec<PathBuf>>),
}

/// Video jobs are already multithreaded inside the encoder; running several
/// at once mostly thrashes memory.
fn is_video(job: &Job) -> bool {
    job.to.category == Category::Video
        || format_by_extension(&job.input).is_some_and(|f| f.category == Category::Video)
}

/// Runs `jobs` with at most `concurrency` at a time, and at most one video
/// job at a time. Results come back in job order. After `cancel`, jobs that
/// haven't started return `Err(Cancelled)`.
pub fn run_batch(
    registry: &Registry,
    jobs: &[Job],
    concurrency: usize,
    cancel: &Cancel,
    on_event: &(dyn Fn(usize, Event) + Sync),
) -> Vec<Result<Vec<PathBuf>>> {
    let results: Vec<Mutex<Option<Result<Vec<PathBuf>>>>> =
        jobs.iter().map(|_| Mutex::new(None)).collect();
    let next = AtomicUsize::new(0);
    let video_lane = Mutex::new(());
    let workers = concurrency.clamp(1, jobs.len().max(1));
    std::thread::scope(|s| {
        for _ in 0..workers {
            s.spawn(|| {
                loop {
                    let i = next.fetch_add(1, Ordering::Relaxed);
                    let Some(job) = jobs.get(i) else { break };
                    let _lane = is_video(job).then(|| video_lane.lock());
                    let result = if cancel.is_cancelled() {
                        Err(Error::Cancelled)
                    } else {
                        on_event(i, Event::Started);
                        registry.run(job, &|p| on_event(i, Event::Progress(p)), cancel)
                    };
                    on_event(i, Event::Finished(&result));
                    *results[i].lock().unwrap() = Some(result);
                }
            });
        }
    });
    results
        .into_iter()
        .map(|r| r.into_inner().unwrap().expect("every job ran"))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expands_folders() {
        let dir = tempfile::tempdir().unwrap();
        let d = dir.path();
        for f in ["b.png", "a.png", ".hidden.png", "sub/c.png", ".git/d.png"] {
            let p = d.join(f);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, b"").unwrap();
        }
        let names = |items: Vec<BatchItem>| -> Vec<String> {
            items
                .into_iter()
                .map(|i| {
                    let rel = i
                        .input
                        .strip_prefix(d)
                        .unwrap()
                        .to_string_lossy()
                        .replace('\\', "/");
                    format!("{rel}{}", if i.explicit { "!" } else { "" })
                })
                .collect()
        };
        let flat = expand_inputs(&[d.to_path_buf(), d.join("b.png")], false).unwrap();
        assert_eq!(names(flat), ["a.png", "b.png", "b.png!"]);
        let deep = expand_inputs(&[d.to_path_buf()], true).unwrap();
        assert_eq!(names(deep.clone()), ["a.png", "b.png", "sub/c.png"]);
        let rel: Vec<PathBuf> = deep.iter().map(BatchItem::relative_dir).collect();
        assert_eq!(rel, [PathBuf::new(), PathBuf::new(), PathBuf::from("sub")]);
    }
}
