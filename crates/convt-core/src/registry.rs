use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::menu::{MenuChoices, default_menu};
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

/// What a subtitle track pulled out of a video may become.
const SUBTITLE_TEXT: &[&str] = &["srt", "vtt", "txt"];

/// Spreadsheets with sheets and formatting, which CSV and TSV flatten.
const WORKBOOKS: &[&str] = &["xlsx", "xls", "ods"];

/// Plain tables: a workbook made into one keeps only its first sheet's values.
const TABLE_TEXT: &[&str] = &["csv", "tsv"];

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
        // Plain text has no structure to turn into Markdown; the route
        // through HTML would only escape its punctuation.
        if from == to || (from.id == "txt" && to.id == "md") {
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
            // Plain text keeps only the words, so it ends a route: a
            // subtitle file reaches TXT but not a PDF or Word file made from it.
            if d == MAX_HOPS || (d > 0 && node == "txt") {
                continue;
            }
            // A workbook reaches CSV, but a Word file, Markdown or JSON made
            // from that CSV would be its first sheet without formatting.
            if d > 0 && TABLE_TEXT.contains(&node) && WORKBOOKS.contains(&from.id) {
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
                // Text comes out of a video only as its own subtitle track, so
                // extraction must be the first hop (a GIF made into an MP4 has
                // none), and the text may only become other subtitle or plain
                // text formats, never a PDF or Word file.
                let video_to_text = edge.1.from.category == Category::Video
                    && edge.1.to.category == Category::Document;
                if video_to_text && d > 0 {
                    continue;
                }
                if from.category == Category::Video
                    && edge.1.from.category == Category::Document
                    && !SUBTITLE_TEXT.contains(&edge.1.to.id)
                {
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
        self.menu_targets_with(from, &MenuChoices::default())
    }

    /// [`menu_targets`](Self::menu_targets), with the lists someone picked
    /// in `choices`. A picked list that `from` can reach none of (an audio
    /// list of only MP3, for an MP3) falls back to the default, so the menu
    /// never comes up empty.
    pub fn menu_targets_with(
        &self,
        from: &'static Format,
        choices: &MenuChoices,
    ) -> Vec<&'static Format> {
        const MENU_SIZE: usize = 4;
        let reachable = self.targets(from);
        let pick = |ids: &mut dyn Iterator<Item = &str>| -> Vec<&'static Format> {
            ids.filter_map(|id| reachable.iter().find(|f| f.id == id).copied())
                .collect()
        };
        if let Some(list) = choices.get(from.category) {
            let picked = pick(&mut list.iter().map(|f| f.id));
            if !picked.is_empty() {
                return picked;
            }
        }
        // Text formats that share a category with Office documents want
        // their own neighbours: a subtitle file wants the other subtitle
        // format, not Word.
        let preferred: &[&str] = match from.id {
            "gif" => &["mp4", "webp", "png"],
            "md" => &["html", "pdf", "docx"],
            "json" => &["yaml", "csv"],
            "yaml" => &["json", "csv"],
            "srt" => &["vtt", "txt"],
            "vtt" => &["srt", "txt"],
            "tsv" => &["csv", "xlsx", "json"],
            _ => default_menu(from.category),
        };
        let picked = pick(&mut preferred.iter().copied());
        if picked.is_empty() {
            reachable.into_iter().take(MENU_SIZE).collect()
        } else {
            picked
        }
    }

    /// Every format some file of `category` can be converted to, sorted like
    /// [`targets`](Self::targets): what a menu for that kind may offer.
    pub fn category_targets(&self, category: Category) -> Vec<&'static Format> {
        let mut out: Vec<&'static Format> = Vec::new();
        for from in crate::FORMATS.iter().filter(|f| f.category == category) {
            for to in self.targets(from) {
                if !out.contains(&to) {
                    out.push(to);
                }
            }
        }
        out.sort_by_key(|f| (f.category as u8, f.name));
        out
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

        let destination = Destination::of(job)?;
        // Staging lives in the destination so publishing is a same-volume
        // rename. Dropping it removes everything on any early return.
        let staging = destination.staging()?;

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
                let ctx = Ctx::new(*step, &job.options, &scaled, cancel)
                    .with_last_hop(i + 1 == plan.hops.len());
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
        let published = destination.publish(&files, &pages)?;
        progress(Some(1.0));
        Ok(published)
    }
}

/// Where a job's results land, by the rules every conversion follows: next
/// to the input or in a folder under the input's name with the target's
/// extension, renamed on collision, or at an exact path. Conversions that
/// don't run through [`Registry::run`] (the desktop app's cloud jobs) use it
/// so their files are named the same way.
#[derive(Debug, Clone)]
pub struct Destination {
    dir: PathBuf,
    stem: String,
    ext: String,
    exact: bool,
}

impl Destination {
    pub fn of(job: &Job) -> Result<Self> {
        let input = &job.input;
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
        Ok(Self {
            dir,
            stem,
            ext,
            exact: matches!(job.output, Output::Exact(_)),
        })
    }

    /// The folder the results go to.
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// A hidden folder inside [`Self::dir`] to write results into before
    /// [`Self::publish`], so publishing is a same-volume rename. It and
    /// whatever is left in it are removed when it is dropped.
    pub fn staging(&self) -> Result<tempfile::TempDir> {
        Ok(tempfile::Builder::new()
            .prefix(".convt-")
            .tempdir_in(&self.dir)?)
    }

    /// Moves `files` from staging to their final names. `pages[i]` is the
    /// 0-based page `files[i]` came from; a single result is page 0.
    pub fn publish(&self, files: &[PathBuf], pages: &[usize]) -> Result<Vec<PathBuf>> {
        publish(files, pages, &self.dir, &self.stem, &self.ext, self.exact)
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
    fn plain_text_ends_a_route_and_never_becomes_markdown() {
        let mut r = Registry::new();
        r.register(Arc::new(Fake(
            "text",
            0,
            vec![
                ("srt", "txt"),
                ("txt", "pdf"),
                ("txt", "html"),
                ("html", "md"),
                ("html", "pdf"),
            ],
        )));
        assert!(r.plan(f("srt"), f("txt")).is_ok());
        assert!(r.plan(f("srt"), f("pdf")).is_err());
        assert!(r.plan(f("txt"), f("pdf")).is_ok());
        assert!(r.plan(f("txt"), f("html")).is_ok());
        assert!(r.plan(f("txt"), f("md")).is_err());
        assert!(r.plan(f("html"), f("md")).is_ok());
    }

    #[test]
    fn a_workbook_flattened_to_csv_goes_no_further() {
        let mut r = Registry::new();
        r.register(Arc::new(Fake(
            "office",
            0,
            vec![
                ("xlsx", "csv"),
                ("xlsx", "pdf"),
                ("csv", "xlsx"),
                ("html", "docx"),
            ],
        )));
        r.register(Arc::new(Fake(
            "data",
            10,
            vec![
                ("csv", "html"),
                ("csv", "json"),
                ("csv", "tsv"),
                ("json", "yaml"),
            ],
        )));
        for to in ["html", "docx", "json", "yaml", "tsv"] {
            assert!(r.plan(f("xlsx"), f(to)).is_err(), "xlsx -> {to}");
        }
        assert!(r.plan(f("xlsx"), f("csv")).is_ok());
        assert!(r.plan(f("xlsx"), f("pdf")).is_ok());
        // A CSV file is already a plain table, so it routes on.
        assert!(r.plan(f("csv"), f("docx")).is_ok());
        assert!(r.plan(f("csv"), f("yaml")).is_ok());
    }

    #[test]
    fn text_formats_have_their_own_menus() {
        let mut r = Registry::new();
        r.register(Arc::new(Fake(
            "text",
            0,
            vec![
                ("md", "html"),
                ("md", "txt"),
                ("html", "pdf"),
                ("html", "docx"),
                ("json", "yaml"),
                ("json", "csv"),
                ("csv", "pdf"),
                ("srt", "vtt"),
                ("srt", "txt"),
                ("txt", "pdf"),
                ("txt", "docx"),
            ],
        )));
        let ids = |from| {
            r.menu_targets(f(from))
                .iter()
                .map(|t| t.id)
                .collect::<Vec<_>>()
        };
        assert_eq!(ids("md"), ["html", "pdf", "docx"]);
        assert_eq!(ids("json"), ["yaml", "csv"]);
        // Reachable through a text file, but not what a subtitle menu offers.
        assert_eq!(ids("srt"), ["vtt", "txt"]);
    }

    #[test]
    fn menus_follow_the_lists_people_pick() {
        let mut r = Registry::new();
        r.register(Arc::new(Fake(
            "audio",
            0,
            vec![
                ("mp3", "wav"),
                ("wav", "mp3"),
                ("wav", "flac"),
                ("wav", "opus"),
                ("wav", "m4a"),
            ],
        )));
        let mut choices = MenuChoices::default();
        let ids = |from, c: &MenuChoices| {
            r.menu_targets_with(f(from), c)
                .iter()
                .map(|t| t.id)
                .collect::<Vec<_>>()
        };
        assert_eq!(ids("mp3", &choices), ["m4a", "wav"]);
        choices.set(
            Category::Audio,
            vec![f("opus"), f("mp3"), f("flac"), f("aac")],
        );
        // In the picked order, without the input's own format or what it
        // can't reach.
        assert_eq!(ids("mp3", &choices), ["opus", "flac"]);
        assert_eq!(ids("wav", &choices), ["opus", "mp3", "flac"]);
        // Nothing picked is reachable: the default, not an empty menu.
        choices.set(Category::Audio, vec![f("mp3")]);
        assert_eq!(ids("mp3", &choices), ["m4a", "wav"]);
        assert_eq!(
            r.category_targets(Category::Audio)
                .iter()
                .map(|t| t.id)
                .collect::<Vec<_>>(),
            ["flac", "m4a", "mp3", "opus", "wav"]
        );
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
    fn video_text_is_its_own_subtitles() {
        let mut r = Registry::new();
        r.register(Arc::new(Fake(
            "ffmpeg",
            0,
            vec![
                ("gif", "mp4"),
                ("avi", "mkv"),
                ("mp4", "srt"),
                ("mkv", "srt"),
            ],
        )));
        r.register(Arc::new(Fake(
            "subtitle",
            0,
            vec![("srt", "vtt"), ("srt", "txt"), ("txt", "srt")],
        )));
        r.register(Arc::new(Fake(
            "office",
            0,
            vec![("txt", "docx"), ("txt", "pdf")],
        )));
        assert!(r.plan(f("mp4"), f("srt")).is_ok());
        assert!(r.plan(f("mp4"), f("vtt")).is_ok());
        assert!(r.plan(f("mkv"), f("txt")).is_ok());
        // A GIF or AVI made into a container has no subtitle track.
        assert!(r.plan(f("gif"), f("srt")).is_err());
        assert!(r.plan(f("avi"), f("vtt")).is_err());
        // Subtitles don't turn a video into a document.
        assert!(r.plan(f("mp4"), f("docx")).is_err());
        assert!(r.plan(f("mp4"), f("pdf")).is_err());
        // Text that didn't come from a video routes as before, and plain
        // text is never a middle hop.
        assert!(r.plan(f("srt"), f("docx")).is_err());
        assert!(r.plan(f("txt"), f("vtt")).is_ok());
    }

    #[test]
    fn only_the_final_hop_is_marked_last() {
        struct Record(&'static str, &'static str, Arc<std::sync::Mutex<Vec<bool>>>);
        impl Engine for Record {
            fn id(&self) -> &'static str {
                "record"
            }
            fn steps(&self) -> Vec<Step> {
                vec![Step {
                    from: f(self.0),
                    to: f(self.1),
                }]
            }
            fn convert(&self, ctx: &Ctx, _: &Path, out_dir: &Path) -> Result<Vec<PathBuf>> {
                self.2.lock().unwrap().push(ctx.is_last_hop());
                let path = ctx.artifact(out_dir, 0);
                std::fs::write(&path, "x")?;
                Ok(vec![path])
            }
        }
        let seen = Arc::new(std::sync::Mutex::new(Vec::new()));
        let mut r = Registry::new();
        r.register(Arc::new(Record("md", "html", seen.clone())))
            .register(Arc::new(Record("html", "pdf", seen.clone())));
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("a.md");
        std::fs::write(&input, "x").unwrap();
        r.run(&Job::new(&input, f("pdf")), &no_progress, &Cancel::new())
            .unwrap();
        assert_eq!(*seen.lock().unwrap(), [false, true]);
        r.run(&Job::new(&input, f("html")), &no_progress, &Cancel::new())
            .unwrap();
        assert_eq!(*seen.lock().unwrap(), [false, true, true]);
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
