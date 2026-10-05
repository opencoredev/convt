use std::path::{Path, PathBuf};
use std::process::Command;

use convt_core::{Ctx, Engine, Error, Options, Result, Step, VideoCodec};

const VIDEO_IN: &[&str] = &["mp4", "mov", "webm", "mkv", "avi", "gif"];
const VIDEO_OUT: &[&str] = &["mp4", "mov", "webm", "mkv", "avi", "gif"];
/// Video to image grabs the first frame.
const FRAME: &[&str] = &["png", "jpeg", "webp"];
const AUDIO: &[&str] = &["mp3", "wav", "flac", "aac", "m4a", "ogg", "opus"];

/// The FFmpeg binary the engine would run, found the way every tool is:
/// `CONVT_FFMPEG`, next to the executable, then `PATH`. The desktop app uses
/// it to grab video thumbnails.
pub fn ffmpeg_path() -> Option<PathBuf> {
    crate::find_tool(&["ffmpeg"], "CONVT_FFMPEG")
}

/// Runs the FFmpeg binary as a child process, so a crash in a codec can't take
/// the app down with it.
pub struct FfmpegEngine {
    ffmpeg: Option<PathBuf>,
    ffprobe: Option<PathBuf>,
}

impl FfmpegEngine {
    pub fn new() -> Self {
        Self {
            ffmpeg: crate::find_tool(&["ffmpeg"], "CONVT_FFMPEG"),
            ffprobe: crate::find_tool(&["ffprobe"], "CONVT_FFPROBE"),
        }
    }

    /// The input's duration for progress, or `None` when ffprobe is missing
    /// or can't tell. Only cancellation is an error.
    fn duration_us(&self, ctx: &Ctx, input: &Path) -> Result<Option<f64>> {
        let Some(ffprobe) = &self.ffprobe else {
            return Ok(None);
        };
        let mut cmd = Command::new(ffprobe);
        cmd.args([
            "-v",
            "error",
            "-show_entries",
            "format=duration",
            "-of",
            "csv=p=0",
        ])
        .arg(input);
        let mut secs = None;
        match crate::run_tool("ffprobe", cmd, ctx, |line| {
            secs = secs.or_else(|| line.trim().parse::<f64>().ok());
        }) {
            Err(Error::Cancelled) => Err(Error::Cancelled),
            Err(_) => Ok(None),
            Ok(()) => Ok(secs.map(|s| s * 1_000_000.0)),
        }
    }
}

impl Default for FfmpegEngine {
    fn default() -> Self {
        Self::new()
    }
}

/// Maps 1-100 quality onto a CRF scale where `best` is quality 100.
fn crf(quality: u8, worst: f32) -> String {
    format!("{:.0}", worst - f32::from(quality) * 0.25)
}

fn video_scale(height: u32) -> String {
    // Zero is a special FFmpeg size sentinel, so clamp narrow frames to two.
    // A one-pixel source or cap cannot satisfy even sizing without enlargement.
    format!(
        "scale='if(lt(iw,2),nan,max(2,trunc(iw*min(1,{height}/ih)/2)*2))':'if(lt(min(ih,{height}),2),nan,max(2,trunc(min(ih,{height})/2)*2))'"
    )
}

/// Encoder arguments for one output format and the user's options.
/// Hardware encoders (VideoToolbox, NVENC, QSV, VAAPI, AMF) get picked here
/// once detection lands.
fn output_args(to: &str, o: &Options) -> Vec<String> {
    let q = o.quality;
    let x264_crf = q.map_or("20".into(), |q| crf(q, 40.0));
    let vp9_crf = q.map_or("32".into(), |q| crf(q, 52.0));
    let aac = format!("{}k", o.audio_bitrate.unwrap_or(192));
    let bitrate = o.audio_bitrate.map(|b| format!("{b}k"));
    let scale = o.video_height.map(video_scale);
    let mut args: Vec<String> = Vec::new();
    let mut push = |a: &[&str]| args.extend(a.iter().map(|s| s.to_string()));
    match to {
        "mp4" | "mov" | "mkv" => {
            match o.video_codec.unwrap_or(VideoCodec::H264) {
                VideoCodec::H264 => {
                    push(&["-c:v", "libx264", "-preset", "medium", "-crf", &x264_crf]);
                }
                VideoCodec::Hevc => {
                    let x265_crf = q.map_or("24".into(), |q| crf(q, 44.0));
                    push(&["-c:v", "libx265", "-preset", "medium", "-crf", &x265_crf]);
                    // QuickTime and Apple devices only play HEVC tagged hvc1.
                    if to != "mkv" {
                        push(&["-tag:v", "hvc1"]);
                    }
                }
            }
            push(&["-pix_fmt", "yuv420p"]);
            if !o.strip_audio {
                push(&["-c:a", "aac", "-b:a", &aac]);
            }
            if to != "mkv" {
                push(&["-movflags", "+faststart"]);
            }
        }
        "webm" => {
            push(&[
                "-c:v",
                "libvpx-vp9",
                "-crf",
                &vp9_crf,
                "-b:v",
                "0",
                "-row-mt",
                "1",
            ]);
            push(&["-deadline", "good", "-cpu-used", "4"]);
            if !o.strip_audio {
                push(&["-c:a", "libopus"]);
                if let Some(b) = &bitrate {
                    push(&["-b:a", b]);
                }
            }
        }
        "avi" => {
            let qv = q.map_or(3, |q| 2 + (100 - u32::from(q)) * 29 / 99);
            push(&["-c:v", "mpeg4", "-q:v", &qv.to_string()]);
            if !o.strip_audio {
                push(&["-c:a", "libmp3lame"]);
                if let Some(b) = &bitrate {
                    push(&["-b:a", b]);
                }
            }
        }
        "gif" => {
            let size = o
                .video_height
                .map_or("scale='min(720,iw)':-2".into(), video_scale);
            let vf = format!(
                "fps=12,{size}:flags=lanczos,split[a][b];[a]palettegen[p];[b][p]paletteuse"
            );
            push(&["-vf", &vf, "-an"]);
            return args;
        }
        "png" | "webp" | "jpeg" => {
            push(&["-frames:v", "1", "-update", "1"]);
            if to == "jpeg" {
                let qv = q.map_or(2, |q| 2 + (100 - u32::from(q)) * 29 / 99);
                push(&["-q:v", &qv.to_string()]);
            }
            if let Some(m) = o.max_size {
                let vf = format!(
                    "scale='min(iw,{m})':'min(ih,{m})':force_original_aspect_ratio=decrease"
                );
                push(&["-vf", &vf]);
            }
            return args;
        }
        "mp3" => match &bitrate {
            Some(b) => push(&["-vn", "-c:a", "libmp3lame", "-b:a", b]),
            None => push(&["-vn", "-c:a", "libmp3lame", "-q:a", "2"]),
        },
        "wav" => push(&["-vn", "-c:a", "pcm_s16le"]),
        "flac" => push(&["-vn", "-c:a", "flac"]),
        "aac" => push(&["-vn", "-c:a", "aac", "-b:a", &aac, "-f", "adts"]),
        "m4a" => push(&["-vn", "-c:a", "aac", "-b:a", &aac]),
        "ogg" => match &bitrate {
            Some(b) => push(&["-vn", "-c:a", "libvorbis", "-b:a", b]),
            None => push(&["-vn", "-c:a", "libvorbis", "-q:a", "5"]),
        },
        "opus" => {
            let b = bitrate.unwrap_or_else(|| "128k".into());
            push(&["-vn", "-c:a", "libopus", "-b:a", &b]);
        }
        _ => {}
    }
    if o.strip_audio && VIDEO_OUT.contains(&to) {
        args.push("-an".into());
    }
    if let Some(vf) = scale.filter(|_| VIDEO_OUT.contains(&to)) {
        args.extend(["-vf".into(), vf]);
    }
    args
}

impl Engine for FfmpegEngine {
    fn id(&self) -> &'static str {
        "ffmpeg"
    }

    fn unavailable_reason(&self) -> Option<String> {
        self.ffmpeg
            .is_none()
            .then(|| "ffmpeg not found (set CONVT_FFMPEG or install it)".into())
    }

    fn steps(&self) -> Vec<Step> {
        let video_targets: Vec<_> = VIDEO_OUT
            .iter()
            .chain(AUDIO)
            .chain(FRAME)
            .copied()
            .collect();
        crate::steps(VIDEO_IN, &video_targets)
            // GIF has no audio stream, so extraction is never meaningful.
            .filter(|step| !(step.from.id == "gif" && AUDIO.contains(&step.to.id)))
            .chain(crate::steps(AUDIO, AUDIO))
            .collect()
    }

    fn convert(&self, ctx: &Ctx, input: &Path, out_dir: &Path) -> Result<Vec<PathBuf>> {
        let ffmpeg = self.ffmpeg.as_ref().ok_or(Error::EngineUnavailable {
            engine: "ffmpeg",
            reason: "ffmpeg not found".into(),
        })?;
        let total = self.duration_us(ctx, input)?.filter(|t| *t > 0.0);
        if total.is_none() {
            ctx.indeterminate();
        }
        let output = ctx.artifact(out_dir, 0);
        let mut cmd = Command::new(ffmpeg);
        cmd.args(["-hide_banner", "-nostdin", "-y", "-v", "error"])
            .args(["-progress", "pipe:1", "-nostats", "-i"])
            .arg(input)
            .args(output_args(ctx.step.to.id, ctx.options))
            .arg(&output);
        crate::run_tool("ffmpeg", cmd, ctx, |line| {
            let us = line
                .strip_prefix("out_time_us=")
                .and_then(|v| v.parse::<f64>().ok());
            if let (Some(us), Some(total)) = (us, total) {
                ctx.progress((us / total) as f32);
            }
        })?;
        Ok(vec![output])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn options_shape_encoder_args() {
        let o = Options {
            quality: Some(80),
            video_height: Some(480),
            audio_bitrate: Some(96),
            ..Options::default()
        };
        let mp4 = output_args("mp4", &o).join(" ");
        assert!(mp4.contains("-crf 20") && mp4.contains("-b:a 96k"), "{mp4}");
        assert!(mp4.ends_with(&format!("-vf {}", video_scale(480))), "{mp4}");
        let gif = output_args("gif", &o).join(" ");
        assert!(
            gif.contains(&format!("{}:flags", video_scale(480))) && !gif.contains("-vf scale"),
            "{gif}"
        );
        let mp3 = output_args("mp3", &o).join(" ");
        assert!(mp3.contains("-b:a 96k") && !mp3.contains("-q:a"), "{mp3}");
        assert!(!output_args("png", &o).join(" ").contains("scale"));
        let best = output_args(
            "jpeg",
            &Options {
                quality: Some(100),
                ..Options::default()
            },
        );
        assert!(best.join(" ").contains("-q:v 2"));
    }

    #[test]
    fn defaults_are_unchanged() {
        let o = Options::default();
        assert_eq!(
            output_args("mp4", &o).join(" "),
            "-c:v libx264 -preset medium -crf 20 -pix_fmt yuv420p -c:a aac -b:a 192k -movflags +faststart"
        );
        assert_eq!(
            output_args("mkv", &o).join(" "),
            "-c:v libx264 -preset medium -crf 20 -pix_fmt yuv420p -c:a aac -b:a 192k"
        );
        assert_eq!(
            output_args("webm", &o).join(" "),
            "-c:v libvpx-vp9 -crf 32 -b:v 0 -row-mt 1 -deadline good -cpu-used 4 -c:a libopus"
        );
        assert_eq!(
            output_args("avi", &o).join(" "),
            "-c:v mpeg4 -q:v 3 -c:a libmp3lame"
        );
    }

    #[test]
    fn codec_and_strip_audio_shape_encoder_args() {
        let hevc = Options {
            video_codec: Some(VideoCodec::Hevc),
            ..Options::default()
        };
        let mp4 = output_args("mp4", &hevc).join(" ");
        assert!(
            mp4.contains("-c:v libx265") && mp4.contains("-tag:v hvc1") && !mp4.contains("libx264"),
            "{mp4}"
        );
        assert!(!output_args("mkv", &hevc).join(" ").contains("hvc1"));
        // The codec only applies where it can: WebM stays VP9.
        assert!(output_args("webm", &hevc).join(" ").contains("libvpx-vp9"));

        let silent = Options {
            strip_audio: true,
            ..Options::default()
        };
        for to in ["mp4", "mov", "mkv", "webm", "avi"] {
            let args = output_args(to, &silent);
            assert!(args.contains(&"-an".to_string()), "{to}: {args:?}");
            assert!(!args.contains(&"-c:a".to_string()), "{to}: {args:?}");
        }
        // Audio targets ignore it rather than producing an empty file.
        assert!(!output_args("mp3", &silent).contains(&"-an".to_string()));
    }

    /// Converts a generated clip with FFmpeg, if it is installed.
    fn convert_clip(to: &str, options: &Options) -> Option<(tempfile::TempDir, PathBuf)> {
        let engine = FfmpegEngine::new();
        if let Some(reason) = engine.unavailable_reason() {
            eprintln!("skipping: {reason}");
            return None;
        }
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("clip.mp4");
        let made = Command::new(engine.ffmpeg.as_ref().unwrap())
            .args([
                "-v",
                "error",
                "-y",
                "-f",
                "lavfi",
                "-i",
                "testsrc=size=128x96:rate=10:duration=1",
            ])
            .args(["-f", "lavfi", "-i", "sine=frequency=440:duration=1"])
            .args([
                "-c:v",
                "libx264",
                "-pix_fmt",
                "yuv420p",
                "-c:a",
                "aac",
                "-shortest",
            ])
            .arg(&input)
            .status()
            .unwrap();
        assert!(made.success(), "could not generate a test clip");
        let step = Step {
            from: convt_core::format_by_id("mp4").unwrap(),
            to: convt_core::format_by_id(to).unwrap(),
        };
        let cancel = convt_core::Cancel::new();
        let ctx = Ctx::new(step, options, &|_| {}, &cancel);
        let out = dir.path().join("out");
        std::fs::create_dir(&out).unwrap();
        let outputs = match engine.convert(&ctx, &input, &out) {
            Ok(outputs) => outputs,
            Err(e) if e.to_string().contains("Unknown encoder") => {
                eprintln!("skipping: this FFmpeg lacks an encoder: {e}");
                return None;
            }
            Err(e) => panic!("{e}"),
        };
        Some((dir, outputs[0].clone()))
    }

    #[test]
    fn video_height_is_a_downscale_cap_with_even_dimensions() {
        for to in ["mov", "gif"] {
            for (height, expected) in [(480, (128, 96)), (48, (64, 48))] {
                let Some((_dir, output)) = convert_clip(
                    to,
                    &Options {
                        video_height: Some(height),
                        ..Options::default()
                    },
                ) else {
                    return;
                };
                let Some(probe) = FfmpegEngine::new().ffprobe else {
                    eprintln!("SKIP resolution check: ffprobe missing");
                    return;
                };
                let result = Command::new(probe)
                    .args([
                        "-v",
                        "error",
                        "-select_streams",
                        "v:0",
                        "-show_entries",
                        "stream=width,height",
                        "-of",
                        "csv=p=0",
                    ])
                    .arg(output)
                    .output()
                    .unwrap();
                assert!(result.status.success());
                assert_eq!(
                    String::from_utf8(result.stdout).unwrap().trim(),
                    format!("{},{}", expected.0, expected.1),
                    "{to}, cap {height}"
                );
            }
        }
    }

    #[test]
    fn video_height_handles_narrow_odd_sources_without_zero_sentinels() {
        let engine = FfmpegEngine::new();
        let Some(ffmpeg) = engine.ffmpeg else {
            eprintln!("SKIP narrow video: FFmpeg missing");
            return;
        };
        for (width, succeeds) in [(3, true), (1, false)] {
            let dir = tempfile::tempdir().unwrap();
            let output = dir.path().join("narrow.mov");
            let result = Command::new(&ffmpeg)
                .args(["-v", "error", "-f", "lavfi", "-i"])
                .arg(format!("testsrc=size={width}x101:rate=1:duration=1"))
                .args([
                    "-vf",
                    &video_scale(48),
                    "-c:v",
                    "libx264",
                    "-pix_fmt",
                    "yuv420p",
                ])
                .arg(&output)
                .output()
                .unwrap();
            if String::from_utf8_lossy(&result.stderr).contains("Unknown encoder") {
                eprintln!("SKIP narrow video: libx264 missing");
                return;
            }
            assert_eq!(result.status.success(), succeeds, "{result:?}");
            if succeeds {
                let Some(probe) = &engine.ffprobe else {
                    eprintln!("SKIP narrow resolution check: ffprobe missing");
                    return;
                };
                let dimensions = Command::new(probe)
                    .args([
                        "-v",
                        "error",
                        "-select_streams",
                        "v:0",
                        "-show_entries",
                        "stream=width,height",
                        "-of",
                        "csv=p=0",
                    ])
                    .arg(output)
                    .output()
                    .unwrap();
                assert!(dimensions.status.success());
                assert_eq!(String::from_utf8(dimensions.stdout).unwrap().trim(), "2,48");
            }
        }
    }

    /// The codec names of each stream, via ffprobe; `None` without it.
    fn streams(path: &Path) -> Option<Vec<String>> {
        let ffprobe = FfmpegEngine::new().ffprobe?;
        let out = Command::new(ffprobe)
            .args([
                "-v",
                "error",
                "-show_entries",
                "stream=codec_type,codec_name",
                "-of",
                "csv=p=0",
            ])
            .arg(path)
            .output()
            .unwrap();
        assert!(out.status.success());
        Some(
            String::from_utf8(out.stdout)
                .unwrap()
                .lines()
                .map(str::to_string)
                .collect(),
        )
    }

    #[test]
    fn real_ffmpeg_honors_codec_and_strip_audio() {
        let cases = [
            (Options::default(), "h264", true),
            (
                Options {
                    video_codec: Some(VideoCodec::Hevc),
                    ..Options::default()
                },
                "hevc",
                true,
            ),
            (
                Options {
                    strip_audio: true,
                    ..Options::default()
                },
                "h264",
                false,
            ),
        ];
        for (options, codec, audio) in cases {
            let Some((_dir, output)) = convert_clip("mov", &options) else {
                return;
            };
            let Some(streams) = streams(&output) else {
                eprintln!("skipping the stream check: ffprobe not found");
                return;
            };
            assert!(
                streams.iter().any(|s| s == &format!("{codec},video")),
                "{streams:?}"
            );
            assert_eq!(
                streams.iter().any(|s| s.ends_with(",audio")),
                audio,
                "{streams:?}"
            );
        }
    }
}
