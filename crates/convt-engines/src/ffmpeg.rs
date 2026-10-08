use std::path::{Path, PathBuf};
use std::process::Command;

use convt_core::{Background, Ctx, Engine, Error, Options, Result, Step, VideoCodec};

const VIDEO_IN: &[&str] = &["mp4", "mov", "webm", "mkv", "avi", "gif"];
const VIDEO_OUT: &[&str] = &["mp4", "mov", "webm", "mkv", "avi", "gif"];
/// Video to image grabs the first frame. WebP goes through PNG and the
/// `image` engine: FFmpeg has no WebP encoder of its own, and the bundled
/// builds leave out libwebp.
const FRAME: &[&str] = &["png", "jpeg"];
const AUDIO: &[&str] = &["mp3", "wav", "flac", "aac", "m4a", "ogg", "opus"];

/// The FFmpeg binary the engine would run, found the way every tool is:
/// `CONVT_FFMPEG`, next to the executable, then `PATH`. The desktop app uses
/// it to grab video thumbnails.
pub fn ffmpeg_path() -> Option<PathBuf> {
    crate::find_tool(&["ffmpeg"], "CONVT_FFMPEG")
}

/// Only self-contained media containers are accepted. Playlist demuxers can
/// open other local files even when network protocols are disabled.
pub const LOCAL_INPUT_ARGS: &[&str] = &[
    "-protocol_whitelist",
    "file,pipe",
    "-format_whitelist",
    "mov,mp4,m4a,3gp,3g2,mj2,matroska,webm,avi,gif,mp3,wav,flac,aac,ogg",
];

fn local_path(path: &Path) -> std::ffi::OsString {
    let mut input = std::ffi::OsString::from("file:");
    input.push(path.as_os_str());
    input
}

/// Build the desktop thumbnail invocation, shared with the local media engine.
pub fn thumbnail_command(ffmpeg: &Path, path: &Path, at: &str, width: u32) -> Command {
    let input = local_path(path);
    let mut command = Command::new(ffmpeg);
    crate::hide_console(&mut command);
    command
        .args(LOCAL_INPUT_ARGS)
        .args(["-nostdin", "-v", "error", "-ss", at, "-i"])
        .arg(input)
        .args(["-frames:v", "1", "-an", "-sn", "-vf"])
        .arg(format!("scale={width}:-2"))
        .args(["-f", "image2pipe", "-c:v", "png", "-"]);
    command
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
        cmd.args(LOCAL_INPUT_ARGS)
            .args([
                "-v",
                "error",
                "-show_entries",
                "format=duration",
                "-of",
                "csv=p=0",
            ])
            .arg(local_path(input));
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

/// FFmpeg writes the extracted frame's (possibly scaled) pixel ratio in pHYs.
/// Read that generated PNG before decoding discards its metadata; this also
/// follows FFmpeg's chosen stream without running another subprocess.
fn png_pixel_aspect(path: &Path) -> Option<(u16, u16)> {
    use std::io::{Read, Seek, SeekFrom};
    let mut file = std::fs::File::open(path).ok()?;
    let mut signature = [0; 8];
    file.read_exact(&mut signature).ok()?;
    if signature != *b"\x89PNG\r\n\x1a\n" {
        return None;
    }
    loop {
        let mut header = [0; 8];
        file.read_exact(&mut header).ok()?;
        let len = u32::from_be_bytes(header[..4].try_into().ok()?);
        match &header[4..] {
            b"pHYs" if len == 9 => {
                let mut data = [0; 9];
                file.read_exact(&mut data).ok()?;
                let num = u32::from_be_bytes(data[..4].try_into().ok()?);
                let den = u32::from_be_bytes(data[4..8].try_into().ok()?);
                if num == 0 || den == 0 {
                    return None;
                }
                let (mut a, mut b) = (num, den);
                while b != 0 {
                    (a, b) = (b, a % b);
                }
                let (num, den) = (num / a, den / a);
                // JFIF has only 16-bit density fields. Scaled frames can need
                // larger ratios; retain their proportions to the nearest
                // representable density instead of falling back to 1:1.
                let scale = f64::from(num.max(den)).max(f64::from(u16::MAX));
                let fit = |value| {
                    (f64::from(value) * f64::from(u16::MAX) / scale)
                        .round()
                        .max(1.) as u16
                };
                return Some((fit(num), fit(den)));
            }
            b"IDAT" | b"IEND" => return None,
            _ => {
                file.seek(SeekFrom::Current(i64::from(len) + 4)).ok()?;
            }
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
            // FFmpeg 9 refuses VP9 in RGB (gbrap, from a transparent GIF),
            // and few players handle it anyway.
            push(&["-deadline", "good", "-cpu-used", "4", "-pix_fmt", "yuv420p"]);
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
        // Still frames are always written as PNG, keeping any alpha; convert()
        // then encodes the target through the image engine.
        "png" => {
            push(&["-frames:v", "1", "-update", "1"]);
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
        let to = ctx.step.to.id;
        if to == "gif" && matches!(ctx.options.background, Some(Background::Color(_))) {
            return Err(Error::InvalidOption(
                "a background color isn't supported for video to GIF yet".into(),
            ));
        }
        // A JPEG frame is written as PNG and encoded by the image engine, so
        // transparency gets the same background handling as any other image
        // (FFmpeg's JPEG encoder dropped it to black). A PNG frame keeps
        // FFmpeg's file, with its color and pixel-aspect tags.
        let via_png = to == "jpeg";
        let frame = out_dir.join("frame.png");
        let written = if via_png { &frame } else { &output };
        let mut cmd = Command::new(ffmpeg);
        cmd.args(["-hide_banner", "-nostdin", "-y", "-v", "error"])
            .args(LOCAL_INPUT_ARGS)
            .args(["-progress", "pipe:1", "-nostats", "-i"])
            .arg(local_path(input))
            .args(output_args(if via_png { "png" } else { to }, ctx.options))
            .arg(written);
        crate::run_tool("ffmpeg", cmd, ctx, |line| {
            let us = line
                .strip_prefix("out_time_us=")
                .and_then(|v| v.parse::<f64>().ok());
            if let (Some(us), Some(total)) = (us, total) {
                ctx.progress((us / total) as f32);
            }
        })?;
        if via_png {
            let img = image::open(&frame).map_err(|e| Error::EngineFailed {
                engine: "ffmpeg",
                message: e.to_string(),
            })?;
            let pixel_aspect = png_pixel_aspect(&frame);
            let _ = std::fs::remove_file(&frame);
            crate::image::encode_with_pixel_aspect(img, to, ctx.options, &output, pixel_aspect)?;
        } else if to == "png" {
            crate::image::background_png(&output, ctx.options)?;
        }
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
            "-c:v libvpx-vp9 -crf 32 -b:v 0 -row-mt 1 -deadline good -cpu-used 4 -pix_fmt yuv420p -c:a libopus"
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

#[cfg(all(test, target_os = "linux"))]
mod security_tests {
    use super::*;
    use std::io::{Read, Write};
    use std::sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    };

    #[test]
    fn local_media_never_fetches_dash_references() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        listener.set_nonblocking(true).unwrap();
        let stop = Arc::new(AtomicBool::new(false));
        let requests = Arc::new(AtomicUsize::new(0));
        let (end, count) = (stop.clone(), requests.clone());
        let thread = std::thread::spawn(move || {
            while !end.load(Ordering::SeqCst) {
                if let Ok((mut stream, _)) = listener.accept() {
                    stream
                        .set_read_timeout(Some(std::time::Duration::from_secs(1)))
                        .unwrap();
                    let _ = stream.read(&mut [0; 2048]);
                    count.fetch_add(1, Ordering::SeqCst);
                    let _ = stream.write_all(
                        b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                    );
                } else {
                    std::thread::sleep(std::time::Duration::from_millis(5));
                }
            }
        });
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("disguised.mp4");
        std::fs::write(&input, format!(r#"<?xml version="1.0"?><MPD xmlns="urn:mpeg:dash:schema:mpd:2011" profiles="urn:mpeg:dash:profile:isoff-on-demand:2011" type="static" mediaPresentationDuration="PT1S" minBufferTime="PT1S"><Period><AdaptationSet mimeType="video/mp4"><Representation id="1" bandwidth="1000"><BaseURL>http://{address}/probe.mp4</BaseURL><SegmentBase indexRange="0-100"><Initialization range="0-100"/></SegmentBase></Representation></AdaptationSet></Period></MPD>"#)).unwrap();
        // The review reproduction used a descriptor without an extension.
        // Keep it open in this process while children read through procfs.
        use std::os::fd::{AsRawFd, FromRawFd};
        let fd = unsafe { libc::memfd_create(c"convt-security-dash".as_ptr(), 0) };
        assert!(fd >= 0);
        let mut memfd = unsafe { std::fs::File::from_raw_fd(fd) };
        memfd.write_all(&std::fs::read(&input).unwrap()).unwrap();
        let memfd_path = PathBuf::from(format!(
            "/proc/{}/fd/{}",
            std::process::id(),
            memfd.as_raw_fd()
        ));
        let options = Options::default();
        let cancel = convt_core::Cancel::new();
        let ctx = Ctx::new(
            Step {
                from: convt_core::format_by_id("mp4").unwrap(),
                to: convt_core::format_by_id("png").unwrap(),
            },
            &options,
            &|_| {},
            &cancel,
        );
        for base in [
            PathBuf::from("/usr/bin"),
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../packaging/out/convt"),
        ] {
            let engine = FfmpegEngine {
                ffmpeg: Some(base.join("ffmpeg")),
                ffprobe: Some(base.join("ffprobe")),
            };
            if !engine.ffmpeg.as_ref().unwrap().exists()
                || !engine.ffprobe.as_ref().unwrap().exists()
            {
                assert_ne!(
                    std::env::var("CONVT_REQUIRE_MEDIA_TOOLS").as_deref(),
                    Ok("1"),
                    "required toolset missing: {base:?}"
                );
                eprintln!("SKIP missing security toolset: {base:?}");
                continue;
            }
            let before = requests.load(Ordering::SeqCst);
            for path in [&input, &memfd_path] {
                let _ = engine.duration_us(&ctx, path);
                assert!(engine.convert(&ctx, path, dir.path()).is_err());
                let out = thumbnail_command(engine.ffmpeg.as_ref().unwrap(), path, "0", 64)
                    .output()
                    .unwrap();
                assert!(!out.status.success());
                assert!(
                    String::from_utf8_lossy(&out.stderr).contains("whitelist"),
                    "{:?}",
                    out.stderr
                );
            }
            eprintln!(
                "{base:?}: {} HTTP requests for probe, conversion and thumbnail, path and memfd",
                requests.load(Ordering::SeqCst) - before
            );
        }
        stop.store(true, Ordering::SeqCst);
        thread.join().unwrap();
        assert_eq!(
            requests.load(Ordering::SeqCst),
            0,
            "local media issued HTTP requests"
        );
    }
}

#[cfg(test)]
mod local_playlist_tests {
    use super::*;

    #[test]
    fn refuses_manifests_that_reference_other_local_files() {
        let dir = tempfile::tempdir().unwrap();
        let segment = dir.path().join("private.mp4");
        std::fs::write(&segment, b"private local file").unwrap();
        let manifests = [
            format!(
                "#EXTM3U\n#EXT-X-TARGETDURATION:1\n#EXT-X-MEDIA-SEQUENCE:0\n#EXTINF:1,\n{}\n#EXT-X-ENDLIST\n",
                segment.display()
            ),
            format!("ffconcat version 1.0\nfile '{}'\n", segment.display()),
            format!(
                r#"<MPD xmlns="urn:mpeg:dash:schema:mpd:2011" profiles="urn:mpeg:dash:profile:isoff-on-demand:2011" type="static" mediaPresentationDuration="PT1S" minBufferTime="PT1S"><Period><AdaptationSet mimeType="video/mp4"><Representation id="1" bandwidth="1000"><BaseURL>{}</BaseURL><SegmentBase indexRange="0-100"><Initialization range="0-100"/></SegmentBase></Representation></AdaptationSet></Period></MPD>"#,
                segment.display()
            ),
        ];
        let mut tools = Vec::new();
        if let Some(probe) = crate::find_tool(&["ffprobe"], "CONVT_FFPROBE") {
            tools.push(probe);
        }
        let bundle =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../packaging/out/convt/ffprobe");
        if bundle.exists() {
            tools.push(bundle);
        }
        for tool in tools {
            let demuxers = Command::new(&tool)
                .args(["-hide_banner", "-demuxers"])
                .output()
                .unwrap();
            assert!(demuxers.status.success(), "{tool:?}: cannot list demuxers");
            let demuxers = String::from_utf8_lossy(&demuxers.stdout);
            for (n, manifest) in manifests.iter().enumerate() {
                let demuxer = ["hls", "concat", "dash"][n];
                if !demuxers
                    .lines()
                    .any(|line| line.split_whitespace().nth(1) == Some(demuxer))
                {
                    eprintln!("SKIP {tool:?}: {demuxer} demuxer unavailable");
                    continue;
                }
                let input = dir
                    .path()
                    .join(["playlist.m3u8", "playlist.ffconcat", "playlist.mpd"][n]);
                std::fs::write(&input, manifest).unwrap();
                let out = Command::new(&tool)
                    .args(LOCAL_INPUT_ARGS)
                    .args(["-v", "error"])
                    .arg(local_path(&input))
                    .output()
                    .unwrap();
                assert!(!out.status.success());
                assert!(
                    String::from_utf8_lossy(&out.stderr).contains("whitelist"),
                    "{tool:?}: {}",
                    String::from_utf8_lossy(&out.stderr)
                );
            }
        }
    }
}
