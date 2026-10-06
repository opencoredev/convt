use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::publish::publish;
use crate::{
    Cancel, Category, Ctx, Engine, Error, Format, Options, Progress, Result, Step,
    format_by_extension,
};

/// The engines chosen for each hop of a conversion.
pub struct Plan {
    pub hops: Vec<(Arc<dyn Engine>, Step)>,
}

impl Plan {
    pub fn describe(&self) -> String {
        let mut out = String::new();
        for (i, (engine, step)) in self.hops.iter().enumerate() {
            if i == 0 {
                out.push_str(step.from.id);
            }
            out.push_str(&format!(" -[{}]-> {}", engine.id(), step.to.id));
        }
        out
    }
}

/// An engine and the step it performs.
type Edge = (Arc<dyn Engine>, Step);

#[derive(Default)]
pub struct Registry {
    engines: Vec<Arc<dyn Engine>>,
    /// For each source format id, the best engine for each target format id.
    edges: HashMap<&'static str, HashMap<&'static str, Edge>>,
    /// Engines that were skipped, with the reason.
    unavailable: Vec<(&'static str, String)>,
}

/// Conversions longer than this are almost always lossy detours.
const MAX_HOPS: usize = 3;

/// Intermediate formats that lose nothing, preferred when routes tie on length.
const LOSSLESS_HOPS: &[&str] = &[
    "png", "tiff", "wav", "flac", "mkv", "pdf", "odt", "ods", "odp",
];

impl Registry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds an engine. Engines that can't run on this machine are skipped.
    pub fn register(&mut self, engine: Arc<dyn Engine>) -> &mut Self {
        if let Some(reason) = engine.unavailable_reason() {
            tracing::info!(engine = engine.id(), %reason, "engine unavailable");
            self.unavailable.push((engine.id(), reason));
            return self;
        }
        for step in engine.steps() {
            let targets = self.edges.entry(step.from.id).or_default();
            let better = targets
                .get(step.to.id)
                .is_none_or(|(current, _)| engine.priority() > current.priority());
            if better {
                targets.insert(step.to.id, (engine.clone(), step));
            }
        }
        self.engines.push(engine);
        self
    }

    pub fn engines(&self) -> &[Arc<dyn Engine>] {
        &self.engines
    }

    pub fn unavailable(&self) -> &[(&'static str, String)] {
        &self.unavailable
    }

    /// Finds the shortest chain of engines from `from` to `to`.
    pub fn plan(&self, from: &'static Format, to: &'static Format) -> Result<Plan> {
        let no_route = || Error::NoRoute {
            from: from.id.into(),
            to: to.id.into(),
        };
        if from == to {
            return Err(no_route());
        }
        let mut prev: HashMap<&str, (&str, &Edge)> = HashMap::new();
        let mut depth: HashMap<&str, usize> = HashMap::from([(from.id, 0)]);
        let mut queue = VecDeque::from([from.id]);
        while let Some(node) = queue.pop_front() {
            if node == to.id {
                break;
            }
            let d = depth[node];
            if d == MAX_HOPS {
                continue;
            }
            // HashMap order changes between runs, so equal-length routes would be
            // picked at random. Visit neighbours in a fixed, preferred order.
            let mut next: Vec<_> = self.edges.get(node).into_iter().flatten().collect();
            next.sort_by_key(|(target, edge)| {
                (
                    -edge.0.priority(),
                    !LOSSLESS_HOPS.contains(target),
                    **target,
                )
            });
            for (target, edge) in next {
                // Only a direct step may turn a still file into video or audio,
                // so an SVG never offers MP4 by way of an animated GIF.
                let media = |f: &Format| matches!(f.category, Category::Video | Category::Audio);
                if d > 0 && media(edge.1.to) && !media(from) {
                    continue;
                }
                if !depth.contains_key(target) {
                    depth.insert(target, d + 1);
                    prev.insert(target, (node, edge));
                    queue.push_back(target);
                }
            }
        }
        let mut hops = Vec::new();
        let mut node = to.id;
        while node != from.id {
            let (parent, edge) = prev.get(node).ok_or_else(no_route)?;
            hops.push((*edge).clone());
            node = parent;
        }
        hops.reverse();
        Ok(Plan { hops })
    }

    /// Every format a file of `from` can be converted to, sorted by category then name.
    pub fn targets(&self, from: &'static Format) -> Vec<&'static Format> {
        let mut out: Vec<_> = crate::FORMATS
            .iter()
            .filter(|to| self.plan(from, to).is_ok())
            .collect();
        out.sort_by_key(|f| (f.category as u8, f.name));
        out
    }

    /// The few targets a right-click menu offers for `from`, most wanted
    /// first. Everything else stays one click away in Quick convert, so a
    /// HEIC photo is offered JPEG, PNG and WebP rather than ICO or QOI. Only
    /// targets [`targets`](Self::targets) reaches are listed; a format with
    /// none of its preferred targets gets the first few it can reach.
    pub fn menu_targets(&self, from: &'static Format) -> Vec<&'static Format> {
        const MENU_SIZE: usize = 4;
        let preferred: &[&str] = match (from.id, from.category) {
            ("gif", _) => &["mp4", "webp", "png"],
            (_, Category::Image) => &["jpeg", "png", "webp"],
            (_, Category::Vector) => &["png", "jpeg", "pdf"],
            (_, Category::Video) => &["mp4", "mov", "gif", "mp3"],
            (_, Category::Audio) => &["mp3", "m4a", "wav"],
            (_, Category::Pdf) => &["png", "jpeg", "docx"],
            (_, Category::Document) => &["pdf", "docx", "txt"],
            (_, Category::Spreadsheet) => &["pdf", "xlsx", "csv"],
            (_, Category::Presentation) => &["pdf", "pptx"],
        };
        let reachable = self.targets(from);
        let picked: Vec<_> = preferred
            .iter()
            .filter_map(|id| reachable.iter().find(|f| f.id == *id).copied())
            .collect();
        if picked.is_empty() {
            reachable.into_iter().take(MENU_SIZE).collect()
        } else {
            picked
        }
    }

    /// Runs a job: routes it, runs every hop on every artifact the previous
    /// hop produced, and publishes the results without replacing any file.
    /// Returns the published paths, one per page for paged output. On error
    /// or cancellation nothing the job wrote is left behind.
    pub fn run(&self, job: &Job, progress: Progress, cancel: &Cancel) -> Result<Vec<PathBuf>> {
        job.options.validate()?;
        let input = &job.input;
        let from =
            format_by_extension(input).ok_or_else(|| Error::UndetectedFormat(input.clone()))?;
        let plan = self.plan(from, job.to)?;
        tracing::debug!(plan = plan.describe(), "converting {}", input.display());

        let (dir, stem, ext) = match &job.output {
            Output::Beside | Output::Dir(_) => {
                let dir = match &job.output {
                    Output::Dir(d) => d.clone(),
                    _ => parent_dir(input),
                };
                let stem = input
                    .file_stem()
                    .ok_or_else(|| Error::UndetectedFormat(input.clone()))?;
                (
                    dir,
                    stem.to_string_lossy().into_owned(),
                    job.to.extension().to_string(),
                )
            }
            // The caller's name wins, extension included (`photo.jpeg`, or
            // none at all), so the first output lands exactly on `path`.
            Output::Exact(path) => {
                let stem = path
                    .file_stem()
                    .ok_or_else(|| Error::OutputExists(path.clone()))?;
                let ext = path.extension().unwrap_or_default();
                (
                    parent_dir(path),
                    stem.to_string_lossy().into_owned(),
                    ext.to_string_lossy().into_owned(),
                )
            }
        };
        // Staging lives in the destination so publishing is a same-volume
        // rename. Dropping it removes everything on any early return.
        let staging = tempfile::Builder::new()
            .prefix(".convt-")
            .tempdir_in(&dir)?;

        let n = plan.hops.len() as f32;
        // Each file in flight carries the 0-based page it came from.
        let mut current = vec![(input.clone(), 0)];
        for (i, (engine, step)) in plan.hops.iter().enumerate() {
            let k = current.len() as f32;
            let mut next = Vec::new();
            for (j, (file, page)) in current.iter().enumerate() {
                if cancel.is_cancelled() {
                    return Err(Error::Cancelled);
                }
                let out_dir = staging.path().join(format!("{i}-{j}"));
                std::fs::create_dir(&out_dir)?;
                let base = i as f32 + j as f32 / k;
                let scaled = |p: Option<f32>| {
                    progress(p.map(|p| (base + p.clamp(0.0, 1.0) / k) / n));
                };
                let ctx = Ctx::new(*step, &job.options, &scaled, cancel);
                let produced = match engine.convert(&ctx, file, &out_dir) {
                    Err(_) if cancel.is_cancelled() => return Err(Error::Cancelled),
                    r => r?,
                };
                if produced.is_empty() || produced.iter().any(|p| !p.starts_with(&out_dir)) {
                    return Err(Error::EngineFailed {
                        engine: engine.id(),
                        message: "produced no output".into(),
                    });
                }
                // Engines name artifacts `1.ext`, `2.ext`, ... by index, or by
                // page when they paginate, so a hop that writes one file keeps
                // its input's page.
                next.extend(produced.into_iter().enumerate().map(|(n, path)| {
                    let index = path
                        .file_stem()
                        .and_then(|s| s.to_str()?.parse::<usize>().ok())
                        .map_or(n, |k| k.saturating_sub(1));
                    (path, page + index)
                }));
                progress(Some((base + 1.0 / k) / n));
            }
            current = next;
        }
        if cancel.is_cancelled() {
            return Err(Error::Cancelled);
        }
        let (files, pages): (Vec<_>, Vec<_>) = current.into_iter().unzip();
        let published = publish(
            &files,
            &pages,
            &dir,
            &stem,
            &ext,
            matches!(job.output, Output::Exact(_)),
        )?;
        progress(Some(1.0));
        Ok(published)
    }
}

fn parent_dir(path: &Path) -> PathBuf {
    match path.parent() {
        Some(p) if !p.as_os_str().is_empty() => p.to_path_buf(),
        _ => PathBuf::from("."),
    }
}

/// Where a job's output goes.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum Output {
    /// Next to the input, renamed on collision.
    #[default]
    Beside,
    /// In this directory, renamed on collision.
    Dir(PathBuf),
    /// At exactly this path (pages get `-2`, `-3`). Fails if it's taken.
    Exact(PathBuf),
}

/// One file to convert.
#[derive(Debug, Clone)]
pub struct Job {
    pub input: PathBuf,
    pub to: &'static Format,
    pub options: Options,
    pub output: Output,
}

impl Job {
    pub fn new(input: impl Into<PathBuf>, to: &'static Format) -> Self {
        Self {
            input: input.into(),
            to,
            options: Options::default(),
            output: Output::Beside,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::format_by_id;

    struct Fake(&'static str, i32, Vec<(&'static str, &'static str)>);

    /// Writes `pages` artifacts, each the input bytes plus a page marker.
    struct Pager {
        id: &'static str,
        step: (&'static str, &'static str),
        pages: usize,
        /// Waits between pages, checking for cancellation.
        delay: std::time::Duration,
    }

    impl Engine for Pager {
        fn id(&self) -> &'static str {
            self.id
        }
        fn steps(&self) -> Vec<Step> {
            vec![Step {
                from: format_by_id(self.step.0).unwrap(),
                to: format_by_id(self.step.1).unwrap(),
            }]
        }
        fn convert(&self, ctx: &Ctx, input: &Path, out_dir: &Path) -> Result<Vec<PathBuf>> {
            let data = std::fs::read(input)?;
            let mut out = Vec::new();
            for page in 0..self.pages {
                std::thread::sleep(self.delay);
                ctx.check()?;
                let path = ctx.artifact(out_dir, page);
                let mut bytes = data.clone();
                bytes.extend(format!("|{}{page}", self.id).bytes());
                std::fs::write(&path, bytes)?;
                out.push(path);
                ctx.progress((page + 1) as f32 / self.pages as f32);
            }
            Ok(out)
        }
    }

    fn pager(id: &'static str, step: (&'static str, &'static str), pages: usize) -> Arc<Pager> {
        Arc::new(Pager {
            id,
            step,
            pages,
            delay: std::time::Duration::ZERO,
        })
    }

    fn no_progress(_: Option<f32>) {}

    /// Files in `dir`, sorted, ignoring nothing (so leftovers show up).
    fn listing(dir: &Path) -> Vec<String> {
        let mut v: Vec<_> = std::fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        v.sort();
        v
    }

    impl Engine for Fake {
        fn id(&self) -> &'static str {
            self.0
        }
        fn priority(&self) -> i32 {
            self.1
        }
        fn steps(&self) -> Vec<Step> {
            self.2
                .iter()
                .map(|(a, b)| Step {
                    from: format_by_id(a).unwrap(),
                    to: format_by_id(b).unwrap(),
                })
                .collect()
        }
        fn convert(&self, ctx: &Ctx, input: &Path, out_dir: &Path) -> Result<Vec<PathBuf>> {
            let out = ctx.artifact(out_dir, 0);
            std::fs::copy(input, &out)?;
            Ok(vec![out])
        }
    }

    fn f(id: &str) -> &'static Format {
        format_by_id(id).unwrap()
    }

    #[test]
    fn picks_direct_route_and_priority() {
        let mut r = Registry::new();
        r.register(Arc::new(Fake("slow", 0, vec![("heic", "jpeg")])));
        r.register(Arc::new(Fake("fast", 10, vec![("heic", "jpeg")])));
        let plan = r.plan(f("heic"), f("jpeg")).unwrap();
        assert_eq!(plan.describe(), "heic -[fast]-> jpeg");
    }

    #[test]
    fn chains_engines() {
        let mut r = Registry::new();
        r.register(Arc::new(Fake("img", 0, vec![("heic", "png")])));
        r.register(Arc::new(Fake("pdf", 0, vec![("png", "pdf")])));
        assert_eq!(
            r.plan(f("heic"), f("pdf")).unwrap().describe(),
            "heic -[img]-> png -[pdf]-> pdf"
        );
        assert!(r.plan(f("pdf"), f("heic")).is_err());
        assert_eq!(
            r.targets(f("heic"))
                .iter()
                .map(|f| f.id)
                .collect::<Vec<_>>(),
            ["png", "pdf"]
        );
    }

    #[test]
    fn equal_length_routes_are_picked_the_same_way_every_time() {
        for _ in 0..50 {
            let mut r = Registry::new();
            r.register(Arc::new(Fake(
                "img",
                0,
                vec![("exr", "jpeg"), ("exr", "tiff"), ("exr", "bmp")],
            )));
            r.register(Arc::new(Fake(
                "heif",
                0,
                vec![("jpeg", "heic"), ("tiff", "heic"), ("bmp", "heic")],
            )));
            assert_eq!(
                r.plan(f("exr"), f("heic")).unwrap().describe(),
                "exr -[img]-> tiff -[heif]-> heic"
            );
        }
    }

    #[test]
    fn menus_offer_the_popular_targets_that_are_reachable() {
        let mut r = Registry::new();
        r.register(Arc::new(Fake(
            "img",
            0,
            vec![
                ("heic", "png"),
                ("png", "jpeg"),
                ("png", "webp"),
                ("png", "gif"),
                ("png", "ico"),
                ("png", "heic"),
            ],
        )));
        r.register(Arc::new(Fake("x", 0, vec![("flac", "opus")])));
        let ids = |from| {
            r.menu_targets(f(from))
                .iter()
                .map(|t| t.id)
                .collect::<Vec<_>>()
        };
        // GIF, ICO and the input's own format stay in Quick convert.
        assert_eq!(ids("heic"), ["jpeg", "png", "webp"]);
        assert_eq!(ids("png"), ["jpeg", "webp"]);
        // No preferred target is reachable: fall back to what is.
        assert_eq!(ids("flac"), ["opus"]);
    }

    #[test]
    fn stills_only_become_video_directly() {
        let mut r = Registry::new();
        r.register(Arc::new(Fake("svg", 0, vec![("svg", "png")])));
        r.register(Arc::new(Fake("img", 0, vec![("png", "gif")])));
        r.register(Arc::new(Fake(
            "ffmpeg",
            0,
            vec![("gif", "mp4"), ("mp4", "webm")],
        )));
        assert!(r.plan(f("svg"), f("mp4")).is_err());
        assert!(r.plan(f("gif"), f("mp4")).is_ok());
        assert!(r.plan(f("gif"), f("webm")).is_err());
    }

    #[test]
    fn run_carries_every_page_through_later_hops() {
        let mut r = Registry::new();
        r.register(pager("pdf", ("pdf", "png"), 3));
        r.register(pager("img", ("png", "webp"), 1));
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("doc.pdf");
        std::fs::write(&input, b"src").unwrap();
        let progress = std::sync::Mutex::new(Vec::new());
        let out = r
            .run(
                &Job::new(&input, f("webp")),
                &|p| progress.lock().unwrap().push(p),
                &Cancel::new(),
            )
            .unwrap();
        assert_eq!(
            listing(dir.path()),
            ["doc-2.webp", "doc-3.webp", "doc.pdf", "doc.webp"]
        );
        for (i, path) in out.iter().enumerate() {
            assert_eq!(
                std::fs::read(path).unwrap(),
                format!("src|pdf{i}|img0").as_bytes()
            );
        }
        let progress = progress.into_inner().unwrap();
        assert_eq!(progress.last(), Some(&Some(1.0)));
        let fractions: Vec<f32> = progress.into_iter().flatten().collect();
        assert!(fractions.windows(2).all(|w| w[0] <= w[1]), "{fractions:?}");
    }

    #[test]
    fn existing_files_and_the_input_are_never_replaced() {
        let mut r = Registry::new();
        r.register(pager("a", ("png", "jpeg"), 1));
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("x.png");
        std::fs::write(&input, b"in").unwrap();
        std::fs::write(dir.path().join("x.jpg"), b"keep").unwrap();
        let out = r
            .run(&Job::new(&input, f("jpeg")), &no_progress, &Cancel::new())
            .unwrap();
        assert_eq!(out, [dir.path().join("x (1).jpg")]);
        assert_eq!(std::fs::read(dir.path().join("x.jpg")).unwrap(), b"keep");

        let exact = Job {
            output: Output::Exact(dir.path().join("x.jpg")),
            ..Job::new(&input, f("jpeg"))
        };
        let err = r.run(&exact, &no_progress, &Cancel::new()).unwrap_err();
        assert_eq!(err.kind(), "output_exists");
        assert_eq!(listing(dir.path()), ["x (1).jpg", "x.jpg", "x.png"]);
    }

    #[test]
    fn exact_output_keeps_the_requested_name() {
        let mut r = Registry::new();
        r.register(pager("a", ("png", "jpeg"), 1));
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("x.png");
        std::fs::write(&input, b"in").unwrap();
        let exact = |name: &str| Job {
            output: Output::Exact(dir.path().join(name)),
            ..Job::new(&input, f("jpeg"))
        };
        for name in ["wanted.jpeg", "bare"] {
            let out = r.run(&exact(name), &no_progress, &Cancel::new()).unwrap();
            assert_eq!(out, [dir.path().join(name)]);
        }
        let err = r
            .run(&exact("wanted.jpeg"), &no_progress, &Cancel::new())
            .unwrap_err();
        assert_eq!(err.kind(), "output_exists");
        assert_eq!(listing(dir.path()), ["bare", "wanted.jpeg", "x.png"]);
    }

    #[test]
    fn simultaneous_jobs_with_the_same_name_both_survive() {
        let mut r = Registry::new();
        r.register(Arc::new(Pager {
            delay: std::time::Duration::from_millis(5),
            ..Arc::into_inner(pager("a", ("png", "jpeg"), 2)).unwrap()
        }));
        let dir = tempfile::tempdir().unwrap();
        let out_dir = dir.path().join("out");
        std::fs::create_dir(&out_dir).unwrap();
        let inputs: Vec<_> = ["a", "b"]
            .iter()
            .map(|sub| {
                let d = dir.path().join(sub);
                std::fs::create_dir(&d).unwrap();
                let p = d.join("same.png");
                std::fs::write(&p, sub.as_bytes()).unwrap();
                p
            })
            .collect();
        let results: Vec<_> = std::thread::scope(|s| {
            let handles: Vec<_> = inputs
                .iter()
                .map(|input| {
                    let job = Job {
                        output: Output::Dir(out_dir.clone()),
                        ..Job::new(input, f("jpeg"))
                    };
                    let r = &r;
                    s.spawn(move || r.run(&job, &no_progress, &Cancel::new()).unwrap())
                })
                .collect();
            handles.into_iter().map(|h| h.join().unwrap()).collect()
        });
        assert_eq!(
            listing(&out_dir),
            ["same (1).jpg", "same-2 (1).jpg", "same-2.jpg", "same.jpg"]
        );
        // Each job's pages stay together under one suffix.
        let mut owners: Vec<_> = results
            .iter()
            .map(|paths| {
                let a = std::fs::read(&paths[0]).unwrap();
                let b = std::fs::read(&paths[1]).unwrap();
                assert_eq!(a[..1], b[..1]);
                a[0]
            })
            .collect();
        owners.sort();
        assert_eq!(owners, [b'a', b'b']);
    }

    #[test]
    fn cancelling_mid_job_leaves_nothing_behind() {
        let mut r = Registry::new();
        r.register(Arc::new(Pager {
            delay: std::time::Duration::from_millis(20),
            ..Arc::into_inner(pager("a", ("pdf", "png"), 50)).unwrap()
        }));
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("doc.pdf");
        std::fs::write(&input, b"src").unwrap();
        let cancel = Cancel::new();
        let err = std::thread::scope(|s| {
            let c = cancel.clone();
            s.spawn(move || {
                std::thread::sleep(std::time::Duration::from_millis(60));
                c.cancel();
            });
            r.run(&Job::new(&input, f("png")), &no_progress, &cancel)
                .unwrap_err()
        });
        assert!(matches!(err, Error::Cancelled));
        assert_eq!(listing(dir.path()), ["doc.pdf"]);
    }

    #[test]
    fn engine_errors_clean_up_too() {
        struct Broken;
        impl Engine for Broken {
            fn id(&self) -> &'static str {
                "broken"
            }
            fn steps(&self) -> Vec<Step> {
                vec![Step {
                    from: format_by_id("png").unwrap(),
                    to: format_by_id("gif").unwrap(),
                }]
            }
            fn convert(&self, ctx: &Ctx, _: &Path, out_dir: &Path) -> Result<Vec<PathBuf>> {
                std::fs::write(ctx.artifact(out_dir, 0), b"half")?;
                Err(Error::EngineFailed {
                    engine: "broken",
                    message: "boom".into(),
                })
            }
        }
        let mut r = Registry::new();
        r.register(pager("a", ("jpeg", "png"), 2));
        r.register(Arc::new(Broken));
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("x.jpg");
        std::fs::write(&input, b"in").unwrap();
        let err = r
            .run(&Job::new(&input, f("gif")), &no_progress, &Cancel::new())
            .unwrap_err();
        assert_eq!(err.kind(), "engine_failed");
        assert_eq!(listing(dir.path()), ["x.jpg"]);
    }
}
