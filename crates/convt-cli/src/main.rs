use std::io::{IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use anyhow::{Context, bail};
use clap::{CommandFactory, Parser, Subcommand};
use convt_core::{
    Background, Cancel, Category, Event, FORMATS, Job, Options, Output, PageRange, Preset,
    VideoCodec, expand_inputs, format_by_extension, format_by_id, run_batch,
};
use convt_license::client::{Config, Licensing};
use serde_json::json;

mod skill;

#[derive(Parser)]
#[command(
    name = "convt",
    bin_name = "convt",
    version,
    about = "Convert files locally",
    after_help = "AI coding agents: run `convt --skill` (or `convt skill`) to print a SKILL.md.",
    args_conflicts_with_subcommands = true
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Cmd>,
    /// Print a SKILL.md for AI coding agents and exit
    #[arg(long)]
    skill: bool,
    /// Files or folders to convert
    files: Vec<PathBuf>,
    /// Target format, e.g. png, mp4, pdf
    #[arg(short, long)]
    to: Option<String>,
    /// Output directory (defaults to each input's directory)
    #[arg(short, long)]
    out_dir: Option<PathBuf>,
    /// A preset name from `convt presets`, or a path to a preset file
    #[arg(short, long)]
    preset: Option<String>,
    /// Lossy quality from 1 to 100 (JPEG, HEIC, AVIF, video)
    #[arg(short, long)]
    quality: Option<u8>,
    /// Longest edge of image output, in pixels
    #[arg(long, value_name = "PX")]
    max_size: Option<u32>,
    /// PDF pages to render: 2, 1-3 or 4-
    #[arg(long)]
    pages: Option<PageRange>,
    /// Resolution for PDF and SVG rendering
    #[arg(long)]
    dpi: Option<u32>,
    /// Maximum output video height in pixels (never enlarges)
    #[arg(long, value_name = "PX")]
    video_height: Option<u32>,
    /// Audio bitrate in kbit/s
    #[arg(long, value_name = "KBPS")]
    audio_bitrate: Option<u32>,
    /// Video encoder for MP4, MOV and MKV: h264 (default) or hevc
    #[arg(long, value_name = "CODEC")]
    video_codec: Option<VideoCodec>,
    /// Leave the audio out of video output
    #[arg(long)]
    no_audio: bool,
    /// What transparent areas become: white (default for JPG), black, transparent, or a color like #ff8800
    #[arg(long, value_name = "COLOR")]
    background: Option<Background>,
    /// How many files to convert at once (default: CPU count; video runs one at a time)
    #[arg(short, long)]
    jobs: Option<usize>,
    /// Include subfolders of folder inputs
    #[arg(short, long)]
    recursive: bool,
    /// Print one JSON event per line on stdout
    #[arg(long)]
    json: bool,
}

#[derive(Subcommand)]
enum Cmd {
    /// List every known format
    Formats {
        /// Print JSON (id, name, category, extensions, mime)
        #[arg(long)]
        json: bool,
    },
    /// List the formats a file can be converted to
    Targets {
        file: PathBuf,
        /// Only the few popular targets right-click menus offer
        #[arg(long)]
        menu: bool,
    },
    /// Show the engines available on this machine
    Engines,
    /// List saved presets
    Presets,
    /// Install, inspect or remove optional conversion packs (network only on install)
    Pack {
        #[command(subcommand)]
        action: PackCmd,
    },
    /// Show this machine's license, or add or remove one
    License {
        #[command(subcommand)]
        action: Option<LicenseCmd>,
    },
    /// Print a SKILL.md for AI coding agents
    Skill,
}

#[derive(Subcommand)]
enum PackCmd {
    /// Show the installed document pack without contacting a server
    Status {
        #[arg(value_parser = ["documents"], default_value = "documents")]
        pack: String,
    },
    /// Download and verify the document pack, only on this explicit command
    Install {
        #[arg(value_parser = ["documents"])]
        pack: String,
        /// Explicit local/test source; requires --sha256 (released builds have a pinned source)
        #[arg(long, requires = "sha256")]
        source: Option<String>,
        #[arg(long, requires = "source")]
        sha256: Option<String>,
    },
    /// Remove the current per-user document pack; leaves system LibreOffice alone
    Remove {
        #[arg(value_parser = ["documents"])]
        pack: String,
    },
}

fn pack(action: PackCmd) -> anyhow::Result<()> {
    use convt_engines::packs;
    match action {
        PackCmd::Status { .. } => match packs::documents_status() {
            Ok(path) => println!("documents: installed ({})", path.display()),
            Err(reason) => {
                let advice = if packs::documents_configured() {
                    "run `convt pack install documents`"
                } else {
                    "install LibreOffice to convert documents"
                };
                println!("documents: not installed ({reason:#}); {advice}")
            }
        },
        PackCmd::Install { source, sha256, .. } => {
            let source = match (source, sha256) {
                (Some(url), Some(sha256)) => packs::Source {
                    url,
                    sha256,
                    version: "explicit-source".into(),
                },
                _ => packs::documents_source(),
            };
            let last = std::sync::Mutex::new(0);
            // Throttle terminal output; the app receives every progress event.

            let path = packs::install_documents(&source, &|event| match event {
                packs::Progress::Download { bytes, total } => {
                    let mut last = last.lock().unwrap();
                    if bytes / (8 * 1024 * 1024) > *last {
                        *last = bytes / (8 * 1024 * 1024);
                        eprintln!(
                            "Downloaded {bytes} bytes{}",
                            total.map(|n| format!(" of {n}")).unwrap_or_default()
                        );
                    }
                }
                packs::Progress::Verifying => eprintln!("Verifying SHA-256…"),
                packs::Progress::Extracting => eprintln!("Installing document pack…"),
                packs::Progress::Installed(_) => {}
            })
            .map_err(|error| {
                let again = "run `convt pack install documents` again; it resumes where it stopped";
                let hint = match packs::failure_kind(&error) {
                    packs::FailureKind::Network | packs::FailureKind::HttpStatus(_) => {
                        format!("The document pack download failed. Check your connection and {again}.")
                    }
                    packs::FailureKind::DiskFull => {
                        format!("The disk is full. Free some space and {again}.")
                    }
                    packs::FailureKind::Permission => {
                        "convt can't write to the document pack folder.".to_string()
                    }
                    packs::FailureKind::Busy => {
                        "Another convt is installing or removing the document pack. Try again when it's done.".to_string()
                    }
                    _ => "The document pack was not installed.".to_string(),
                };
                error.context(hint)
            })?;
            println!("documents: installed ({})", path.display());
        }
        PackCmd::Remove { .. } => {
            packs::remove_documents()?;
            println!("documents: removed");
        }
    }
    Ok(())
}

#[derive(Subcommand)]
enum LicenseCmd {
    /// Show the license or trial status (the default)
    Status,
    /// Add a license key. Without KEY, reads it from standard input.
    Activate { key: Option<String> },
    /// Remove the license from this machine
    Remove,
}

fn licensing() -> Licensing {
    use convt_engines::paths;
    Licensing::new(Config::from_env(paths::config_dir(), paths::data_dir()))
}

fn license(action: LicenseCmd) -> anyhow::Result<()> {
    let mut licensing = licensing();
    match action {
        LicenseCmd::Status => println!("{}", licensing.state().summary()),
        LicenseCmd::Activate { key } => {
            if !licensing.enforced() {
                println!("{}", licensing.state().summary());
                return Ok(());
            }
            let key = match key {
                Some(key) => key,
                None => std::io::read_to_string(std::io::stdin())?,
            };
            licensing.activate(&key)?;
            println!("{}", licensing.state().summary());
        }
        LicenseCmd::Remove => {
            licensing
                .deactivate()
                .map_err(|e| anyhow::anyhow!("could not remove the license: {e}"))?;
            println!("Removed the license from this machine.");
        }
    }
    Ok(())
}

fn load_preset(name: &str) -> anyhow::Result<Preset> {
    let path = Path::new(name);
    if path.extension().is_some_and(|e| e == "toml") || name.contains(std::path::MAIN_SEPARATOR) {
        return Ok(Preset::load(path)?);
    }
    let dir = convt_engines::paths::presets_dir().context("no config directory on this system")?;
    let path = dir.join(format!("{name}.toml"));
    if !path.is_file() {
        bail!(
            "no preset named {name:?} in {} (see `convt presets`)",
            dir.display()
        );
    }
    Ok(Preset::load(&path)?)
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    if cli.skill || matches!(cli.command, Some(Cmd::Skill)) {
        skill::print(&Cli::command());
        return Ok(());
    }
    let registry = convt_engines::default_registry();
    match cli.command {
        Some(Cmd::Formats { json: true }) => {
            let list: Vec<_> = FORMATS
                .iter()
                .map(|f| {
                    serde_json::json!({
                        "id": f.id,
                        "name": f.name,
                        "category": format!("{:?}", f.category).to_lowercase(),
                        "extensions": f.extensions,
                        "mime": f.mime,
                    })
                })
                .collect();
            println!("{}", serde_json::to_string_pretty(&list)?);
        }
        Some(Cmd::Formats { json: false }) => {
            for f in FORMATS {
                println!("{:<6} {:<13} {}", f.id, format!("{:?}", f.category), f.name);
            }
        }
        Some(Cmd::Targets { file, menu }) => {
            let from = format_by_extension(&file)
                .with_context(|| format!("unknown format: {}", file.display()))?;
            let targets = if menu {
                registry.menu_targets(from)
            } else {
                registry.targets(from)
            };
            for f in targets {
                println!("{}", f.id);
            }
        }
        Some(Cmd::Engines) => {
            for e in registry.engines() {
                println!("{}", e.id());
            }
            for (id, reason) in registry.unavailable() {
                println!("{id} (unavailable: {reason})");
            }
        }
        Some(Cmd::Presets) => {
            let dir = convt_engines::paths::presets_dir()
                .context("no config directory on this system")?;
            let all = Preset::load_dir(&dir)?;
            if all.is_empty() {
                eprintln!("no presets in {}", dir.display());
            }
            for (name, preset) in all {
                match preset {
                    Ok(p) => println!("{name:<16} {}", p.to.as_deref().unwrap_or("-")),
                    Err(e) => println!("{name:<16} invalid: {e}"),
                }
            }
        }
        Some(Cmd::Pack { action }) => pack(action)?,
        Some(Cmd::License { action }) => license(action.unwrap_or(LicenseCmd::Status))?,
        Some(Cmd::Skill) => skill::print(&Cli::command()),
        None => return convert(cli, &registry),
    }
    Ok(())
}

fn convert(cli: Cli, registry: &convt_core::Registry) -> anyhow::Result<()> {
    let preset = cli.preset.as_deref().map(load_preset).transpose()?;
    let Some(to) = cli
        .to
        .or_else(|| preset.as_ref().and_then(|p| p.to.clone()))
    else {
        bail!("pass --to <format>, or see `convt --help`")
    };
    if cli.files.is_empty() {
        bail!("no input files");
    }
    let to = format_by_id(&to).with_context(|| format!("unknown format: {to}"))?;
    let options = Options {
        quality: cli.quality,
        max_size: cli.max_size,
        video_height: cli.video_height,
        audio_bitrate: cli.audio_bitrate,
        pages: cli.pages,
        dpi: cli.dpi,
        video_codec: cli.video_codec,
        strip_audio: cli.no_audio,
        background: cli.background,
    }
    .or(&preset.map(|p| p.options).unwrap_or_default());
    options.validate()?;
    if options.background == Some(Background::Transparent)
        && to.category == Category::Image
        && !to.keeps_transparency()
    {
        bail!(
            "{} can't store transparency; choose a background color such as white or black",
            to.name
        );
    }

    // Files found in a folder are skipped quietly when they can't become `to`;
    // files named directly are always attempted, so their errors show.
    let jobs: Vec<Job> = expand_inputs(&cli.files, cli.recursive)?
        .into_iter()
        .filter(|item| {
            item.explicit
                || format_by_extension(&item.input)
                    .is_some_and(|from| from != to && registry.plan(from, to).is_ok())
        })
        .map(|item| {
            // With -o, files found in subfolders keep their relative folder so
            // same-named files from different folders don't collide.
            let output = match &cli.out_dir {
                Some(dir) => {
                    let dir = dir.join(item.relative_dir());
                    std::fs::create_dir_all(&dir)
                        .with_context(|| format!("creating {}", dir.display()))?;
                    Output::Dir(dir)
                }
                None => Output::Beside,
            };
            Ok(Job {
                options: options.clone(),
                output,
                ..Job::new(item.input, to)
            })
        })
        .collect::<anyhow::Result<_>>()?;
    if jobs.is_empty() {
        bail!("nothing in those folders converts to {}", to.id);
    }
    if let Err(blocked) = licensing().begin_conversion() {
        bail!("{blocked}");
    }

    let cancel = Cancel::new();
    let on_signal = cancel.clone();
    ctrlc::set_handler(move || {
        if on_signal.is_cancelled() {
            std::process::exit(130);
        }
        on_signal.cancel();
    })?;

    let concurrency = cli
        .jobs
        .unwrap_or_else(|| std::thread::available_parallelism().map_or(4, std::num::NonZero::get));
    let reporter = Reporter::new(&jobs, cli.json);
    let results = run_batch(registry, &jobs, concurrency, &cancel, &|i, e| {
        reporter.event(i, e)
    });
    let cancelled = results
        .iter()
        .filter(|r| matches!(r, Err(convt_core::Error::Cancelled)))
        .count();
    let failed = results.iter().filter(|r| r.is_err()).count() - cancelled;
    reporter.summary(results.len() - failed - cancelled, failed, cancelled);
    if cancel.is_cancelled() {
        std::process::exit(130);
    }
    if failed > 0 {
        std::process::exit(1);
    }
    Ok(())
}

/// Prints batch events: JSON lines on stdout with `--json`, otherwise a
/// result line per file on stderr and a live percentage on a terminal.
struct Reporter<'a> {
    jobs: &'a [Job],
    json: bool,
    tty: bool,
    /// Last whole percentage printed per job, to keep output small.
    last: Mutex<Vec<Option<i32>>>,
}

impl<'a> Reporter<'a> {
    fn new(jobs: &'a [Job], json: bool) -> Self {
        Self {
            jobs,
            json,
            tty: !json && std::io::stderr().is_terminal(),
            last: Mutex::new(vec![None; jobs.len()]),
        }
    }

    fn event(&self, i: usize, event: Event) {
        let input = self.jobs[i].input.display().to_string();
        match event {
            Event::Started => {
                if self.json {
                    emit(json!({"event": "started", "index": i, "input": input}));
                }
            }
            Event::Progress(p) => {
                let pct = p.map(|p| (p * 100.0).floor() as i32);
                {
                    let mut last = self.last.lock().unwrap();
                    if last[i] == Some(pct.unwrap_or(-1)) {
                        return;
                    }
                    last[i] = Some(pct.unwrap_or(-1));
                }
                if self.json {
                    emit(json!({"event": "progress", "index": i, "fraction": p}));
                } else if self.tty {
                    let shown = pct.map_or("...".into(), |p| format!("{p:>3}%"));
                    eprint!("\r\x1b[2K{input}: {shown}");
                    let _ = std::io::stderr().flush();
                }
            }
            Event::Finished(Ok(outputs)) => {
                if self.json {
                    emit(json!({"event": "done", "index": i, "input": input, "outputs": outputs}));
                } else {
                    let list: Vec<_> = outputs.iter().map(|o| o.display().to_string()).collect();
                    self.line(&format!("{input} -> {}", list.join(", ")));
                }
            }
            Event::Finished(Err(e)) => {
                if self.json {
                    emit(json!({
                        "event": "failed",
                        "index": i,
                        "input": input,
                        "kind": e.kind(),
                        "message": e.to_string(),
                    }));
                } else {
                    self.line(&format!("{input}: {e}"));
                }
            }
        }
    }

    fn line(&self, text: &str) {
        if self.tty {
            eprintln!("\r\x1b[2K{text}");
        } else {
            eprintln!("{text}");
        }
    }

    fn summary(&self, done: usize, failed: usize, cancelled: usize) {
        if self.json {
            emit(json!({
                "event": "summary",
                "done": done,
                "failed": failed,
                "cancelled": cancelled,
            }));
        } else if failed > 0 || cancelled > 0 || self.jobs.len() > 1 {
            let mut text = format!("{done} converted, {failed} failed");
            if cancelled > 0 {
                text.push_str(&format!(", {cancelled} cancelled"));
            }
            eprintln!("{text}");
        }
    }
}

fn emit(value: serde_json::Value) {
    let mut out = std::io::stdout().lock();
    let _ = writeln!(out, "{value}");
    let _ = out.flush();
}

#[cfg(test)]
mod skill_sync_tests {
    use super::*;
    use convt_core::FORMATS;

    fn rendered() -> String {
        skill::render(&Cli::command())
    }

    #[test]
    fn skill_frontmatter_and_no_emails() {
        let text = rendered();
        assert!(text.starts_with("---\nname: convt\n"));
        assert!(text.contains("\ndescription:"));
        assert!(text.contains("\n---\n"));
        assert!(
            !text.contains('@'),
            "skill must not contain email addresses"
        );
    }

    #[test]
    fn documented_subcommands_and_flags_exist() {
        let cmd = Cli::command();
        skill::assert_documented_cli_exists(&cmd, &skill::render(&cmd));
    }

    #[test]
    fn every_cli_flag_and_subcommand_is_documented() {
        let cmd = Cli::command();
        skill::assert_cli_is_documented(&cmd, &skill::render(&cmd));
    }

    #[test]
    fn every_format_id_is_listed() {
        let text = rendered();
        for format in FORMATS {
            assert!(
                text.contains(&format!("`{}`", format.id)),
                "format {} missing from skill",
                format.id
            );
        }
    }
}
