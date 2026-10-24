use std::path::{Path, PathBuf};
use std::process::Command;

use convt_core::{Background, Ctx, Engine, Error, Options, Result, Step, Timestamp, VideoCodec};

const VIDEO_IN: &[&str] = &["mp4", "mov", "webm", "mkv", "avi", "gif"];
const VIDEO_OUT: &[&str] = &["mp4", "mov", "webm", "mkv", "avi", "gif"];
/// Video to image grabs the first frame. WebP goes through PNG and the
/// `image` engine: FFmpeg has no WebP encoder of its own, and the bundled
/// builds leave out libwebp.
const FRAME: &[&str] = &["png", "jpeg"];
const AUDIO: &[&str] = &["mp3", "wav", "flac", "aac", "m4a", "ogg", "opus"];
/// Formats re-encoded into themselves, to shrink, trim, strip or retime a
/// file. GIF is here so its animation survives; the image engine would keep
/// only the first frame.
const SAME: &[&str] = &[
    "mp4", "mov", "webm", "mkv", "gif", "mp3", "m4a", "ogg", "opus", "flac",
];

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

/// Targets are offered per format, so a silent video still lists audio ones.
/// Name the cause instead of passing on FFmpeg's empty-output message.
fn no_audio_error(e: Error, to: &str) -> Error {
    match e {
        Error::EngineFailed { message, .. }
            if AUDIO.contains(&to) && message.contains("does not contain any stream") =>
        {
            Error::EngineFailed {
                engine: "ffmpeg",
                message: "this file has no audio track to convert".into(),
            }
        }
        e => e,
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

/// Bitrates or a GIF scale picked to meet `target_size` on one encode
/// attempt. The default keeps the quality-based arguments.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
struct Budget {
    video_kbps: Option<u32>,
    audio_kbps: Option<u32>,
    /// Extra GIF downscale factor, applied after the usual size cap.
    gif_scale: Option<f64>,
}

/// The bitrates each sizable audio encoder accepts, in kbit/s. Vorbis
/// refuses much under 48 for 44.1 kHz stereo.
fn audio_range(to: &str) -> Option<(f64, f64)> {
    match to {
        "mp3" => Some((8.0, 320.0)),
        "aac" | "m4a" => Some((8.0, 320.0)),
        "ogg" => Some((48.0, 320.0)),
        "opus" => Some((6.0, 256.0)),
        _ => None,
    }
}

/// Splits `total_kbps` between video and audio for `to`, or says why a
/// `target` of bytes can't hold `secs` of it.
fn budget(to: &str, o: &Options, total_kbps: f64, target: u64, secs: f64) -> Result<Budget> {
    let too_small = |kind: &str| {
        Error::InvalidOption(format!(
            "{} is too small for {secs:.1} s of {kind}; pick a larger size or trim it",
            convt_core::format_size(target)
        ))
    };
    if to == "gif" || FRAME.contains(&to) {
        return Ok(Budget::default());
    }
    if VIDEO_OUT.contains(&to) {
        let audio = if o.strip_audio {
            None
        } else {
            Some(
                o.audio_bitrate
                    .unwrap_or((total_kbps / 8.0).clamp(32.0, 128.0) as u32),
            )
        };
        let video = total_kbps - f64::from(audio.unwrap_or(0));
        if video < 50.0 {
            return Err(too_small("video"));
        }
        return Ok(Budget {
            video_kbps: Some(video as u32),
            audio_kbps: audio,
            gif_scale: None,
        });
    }
    let Some((min, max)) = audio_range(to) else {
        let name = convt_core::format_by_id(to).map_or(to, |f| f.name);
        return Err(Error::InvalidOption(format!(
            "{name} is lossless, so a target size can't apply; convert to MP3 or M4A instead"
        )));
    };
    if total_kbps < min {
        return Err(too_small("audio"));
    }
    Ok(Budget {
        audio_kbps: Some(total_kbps.min(max) as u32),
        ..Budget::default()
    })
}

/// Encoder arguments for one output format and the user's options.
/// Hardware encoders (VideoToolbox, NVENC, QSV, VAAPI, AMF) get picked here
/// once detection lands. A `budget` from `target_size` replaces CRF and
/// quality scales with bitrates, so an explicit `quality` is ignored then.
fn output_args(to: &str, o: &Options, budget: &Budget) -> Vec<String> {
    let q = o.quality;
    let x264_crf = q.map_or("20".into(), |q| crf(q, 40.0));
    let vp9_crf = q.map_or("32".into(), |q| crf(q, 52.0));
    let audio_kbps = budget.audio_kbps.or(o.audio_bitrate);
    let aac = format!("{}k", audio_kbps.unwrap_or(192));
    let bitrate = audio_kbps.map(|b| format!("{b}k"));
    let rate = budget
        .video_kbps
        .map(|v| (format!("{v}k"), format!("{}k", v * 2)));
    let scale = o.video_height.map(video_scale);
    let mut args: Vec<String> = Vec::new();
    let mut push = |a: &[&str]| args.extend(a.iter().map(|s| s.to_string()));
    if o.strip_metadata {
        push(&["-map_metadata", "-1", "-map_chapters", "-1"]);
    }
    match to {
        "mp4" | "mov" | "mkv" => {
            let (codec, crf) = match o.video_codec.unwrap_or(VideoCodec::H264) {
                VideoCodec::H264 => ("libx264", x264_crf),
                VideoCodec::Hevc => ("libx265", q.map_or("24".into(), |q| crf(q, 44.0))),
            };
            match &rate {
                Some((v, buf)) => push(&[
                    "-c:v", codec, "-preset", "medium", "-b:v", v, "-maxrate", v, "-bufsize", buf,
                ]),
                None => push(&["-c:v", codec, "-preset", "medium", "-crf", &crf]),
            }
            // QuickTime and Apple devices only play HEVC tagged hvc1.
            if codec == "libx265" && to != "mkv" {
                push(&["-tag:v", "hvc1"]);
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
            match &rate {
                Some((v, buf)) => push(&[
                    "-c:v",
                    "libvpx-vp9",
                    "-b:v",
                    v,
                    "-maxrate",
                    v,
                    "-bufsize",
                    buf,
                    "-row-mt",
                    "1",
                ]),
                None => push(&[
                    "-c:v",
                    "libvpx-vp9",
                    "-crf",
                    &vp9_crf,
                    "-b:v",
                    "0",
                    "-row-mt",
                    "1",
                ]),
            }
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
            match &rate {
                Some((v, buf)) => {
                    push(&["-c:v", "mpeg4", "-b:v", v, "-maxrate", v, "-bufsize", buf]);
                }
                None => {
                    let qv = q.map_or(3, |q| 2 + (100 - u32::from(q)) * 29 / 99);
                    push(&["-c:v", "mpeg4", "-q:v", &qv.to_string()]);
                }
            }
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
            // A target size shrinks the frame further; 16 pixels is the floor.
            let shrink = budget.gif_scale.map_or(String::new(), |f| {
                format!(",scale='max(16,trunc(iw*{f:.4}/2)*2)':-2:flags=lanczos")
            });
            let vf = format!(
                "fps={},{size}:flags=lanczos{shrink},split[a][b];[a]palettegen[p];[b][p]paletteuse",
                o.fps.unwrap_or(12)
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
        // FFmpeg 9 otherwise keeps the first decoded frame's length as the
        // block size: 47 samples from an MP3 after gapless trimming, which
        // Apple's decoder refuses to open.
        "flac" => push(&["-vn", "-c:a", "flac", "-frame_size", "4096"]),
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
    if VIDEO_OUT.contains(&to) {
        if o.strip_audio {
            args.push("-an".into());
        }
        // One chain: a second -vf would replace the first.
        let filters: Vec<String> = o
            .fps
            .map(|f| format!("fps={f}"))
            .into_iter()
            .chain(scale)
            .collect();
        if !filters.is_empty() {
            args.extend(["-vf".into(), filters.join(",")]);
        }
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
        let same = SAME.iter().map(|id| {
            let format = convt_core::format_by_id(id).expect("known format");
            Step {
                from: format,
                to: format,
            }
        });
        crate::steps(VIDEO_IN, &video_targets)
            // GIF has no audio stream, so extraction is never meaningful.
            .filter(|step| !(step.from.id == "gif" && AUDIO.contains(&step.to.id)))
            .chain(crate::steps(AUDIO, AUDIO))
            .chain(same)
            .collect()
    }

    fn convert(&self, ctx: &Ctx, input: &Path, out_dir: &Path) -> Result<Vec<PathBuf>> {
        let ffmpeg = self.ffmpeg.as_ref().ok_or(Error::EngineUnavailable {
            engine: "ffmpeg",
            reason: "ffmpeg not found".into(),
        })?;
        let o = ctx.options;
        let to = ctx.step.to.id;
        // A JPEG frame is written as PNG and encoded by the image engine, so
        // transparency gets the same background handling as any other image
        // (FFmpeg's JPEG encoder dropped it to black). A PNG frame keeps
        // FFmpeg's file, with its color and pixel-aspect tags.
        let still = FRAME.contains(&to);
        let duration = self
            .duration_us(ctx, input)?
            .filter(|t| *t > 0.0)
            .map(|us| us / 1_000_000.0);
        let start = o.trim_start.map_or(0.0, Timestamp::as_secs_f64);
        if let (Some(at), Some(duration)) = (o.trim_start, duration)
            && start >= duration
        {
            return Err(Error::InvalidOption(format!(
                "trim start {at} s is past the end of the input ({duration:.1} s)"
            )));
        }
        // The length of the output: what's left after the trim. Without
        // ffprobe, a trim end still bounds it.
        let end = o.trim_end.map(Timestamp::as_secs_f64);
        let secs = match (duration, end) {
            (Some(d), end) => Some(end.map_or(d, |e| e.min(d)) - start),
            (None, Some(e)) => Some(e - start),
            (None, None) => None,
        }
        .filter(|s| *s > 0.0);
        if secs.is_none() {
            ctx.indeterminate();
        }
        let output = ctx.artifact(out_dir, 0);
        if to == "gif" && matches!(o.background, Some(Background::Color(_))) {
            return Err(Error::InvalidOption(
                "a background color isn't supported for video to GIF yet".into(),
            ));
        }
        // Still frames leave target_size to the image engine's encoder.
        let target = o.target_size.filter(|_| !still);
        let mut total_kbps = 0.0;
        let mut plan = Budget::default();
        let target = match target {
            Some(target) => {
                let secs = secs.ok_or_else(|| {
                    Error::InvalidOption("a target size needs ffprobe to read the duration".into())
                })?;
                total_kbps = target as f64 * 8.0 * 0.95 / secs / 1000.0;
                plan = budget(to, o, total_kbps, target, secs)?;
                Some((target, secs))
            }
            None => None,
        };
        // Seeking on the input is fast and frame-accurate when re-encoding;
        // `-to` there is a position in the input, not a length.
        let mut seek: Vec<String> = Vec::new();
        if let Some(at) = o.trim_start {
            seek.extend(["-ss".into(), at.to_string()]);
        }
        if let Some(at) = o.trim_end.filter(|_| !still) {
            seek.extend(["-to".into(), at.to_string()]);
        }
        let via_png = to == "jpeg";
        let frame = out_dir.join("frame.png");
        let written = if via_png { &frame } else { &output };
        let encode = |budget: &Budget, from: f32, until: f32| {
            let mut cmd = Command::new(ffmpeg);
            cmd.args(["-hide_banner", "-nostdin", "-y", "-v", "error"])
                .args(LOCAL_INPUT_ARGS)
                .args(["-progress", "pipe:1", "-nostats"])
                .args(&seek)
                .arg("-i")
                .arg(local_path(input))
                .args(output_args(if via_png { "png" } else { to }, o, budget))
                .arg(written);
            crate::run_tool("ffmpeg", cmd, ctx, |line| {
                let us = line
                    .strip_prefix("out_time_us=")
                    .and_then(|v| v.parse::<f64>().ok());
                if let (Some(us), Some(secs)) = (us, secs) {
                    let done = (us / (secs * 1_000_000.0)).clamp(0.0, 1.0) as f32;
                    ctx.progress(from + (until - from) * done);
                }
            })
            .map_err(|e| no_audio_error(e, to))
        };
        match target {
            None => encode(&plan, 0.0, 1.0)?,
            Some((target, target_secs)) => {
                // Bitrate control overshoots on short or noisy clips, and a
                // GIF has no bitrate at all, so measure and retry smaller.
                // Each attempt fills most of the progress that's left.
                let attempts = if to == "gif" { 6 } else { 3 };
                let mut from = 0.0;
                let mut size = 0;
                for attempt in 0..attempts {
                    ctx.check()?;
                    let until = from + (1.0 - from) * 0.9;
                    encode(&plan, from, until)?;
                    from = until;
                    size = std::fs::metadata(&output)?.len();
                    if size <= target || attempt + 1 == attempts {
                        break;
                    }
                    let ratio = target as f64 / size as f64;
                    if to == "gif" {
                        plan.gif_scale = Some(plan.gif_scale.unwrap_or(1.0) * ratio.sqrt() * 0.9);
                    } else {
                        total_kbps *= ratio * 0.95;
                        plan = budget(to, o, total_kbps, target, target_secs)?;
                    }
                }
                if size > target {
                    let _ = std::fs::remove_file(&output);
                    return Err(Error::InvalidOption(format!(
                        "couldn't fit the {} under {} (the smallest try was {}); pick a larger size{}",
                        ctx.step.to.name,
                        convt_core::format_size(target),
                        convt_core::format_size(size),
                        if to == "gif" {
                            ", a lower fps or a shorter trim"
                        } else {
                            " or trim it"
                        }
                    )));
                }
            }
        }
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
        let mp4 = output_args("mp4", &o, &Budget::default()).join(" ");
        assert!(mp4.contains("-crf 20") && mp4.contains("-b:a 96k"), "{mp4}");
        assert!(mp4.ends_with(&format!("-vf {}", video_scale(480))), "{mp4}");
        let gif = output_args("gif", &o, &Budget::default()).join(" ");
        assert!(
            gif.contains(&format!("{}:flags", video_scale(480))) && !gif.contains("-vf scale"),
            "{gif}"
        );
        let mp3 = output_args("mp3", &o, &Budget::default()).join(" ");
        assert!(mp3.contains("-b:a 96k") && !mp3.contains("-q:a"), "{mp3}");
        assert!(
            !output_args("png", &o, &Budget::default())
                .join(" ")
                .contains("scale")
        );
    }

    #[test]
    fn silent_video_to_audio_names_the_missing_track() {
        let empty = || Error::EngineFailed {
            engine: "ffmpeg",
            message: "Output file does not contain any stream".into(),
        };
        assert!(
            no_audio_error(empty(), "mp3")
                .to_string()
                .contains("no audio track")
        );
        assert!(
            no_audio_error(empty(), "mp4")
                .to_string()
                .contains("does not contain")
        );
    }

    #[test]
    fn defaults_are_unchanged() {
        let o = Options::default();
        assert_eq!(
            output_args("mp4", &o, &Budget::default()).join(" "),
            "-c:v libx264 -preset medium -crf 20 -pix_fmt yuv420p -c:a aac -b:a 192k -movflags +faststart"
        );
        assert_eq!(
            output_args("mkv", &o, &Budget::default()).join(" "),
            "-c:v libx264 -preset medium -crf 20 -pix_fmt yuv420p -c:a aac -b:a 192k"
        );
        assert_eq!(
            output_args("webm", &o, &Budget::default()).join(" "),
            "-c:v libvpx-vp9 -crf 32 -b:v 0 -row-mt 1 -deadline good -cpu-used 4 -pix_fmt yuv420p -c:a libopus"
        );
        assert_eq!(
            output_args("avi", &o, &Budget::default()).join(" "),
            "-c:v mpeg4 -q:v 3 -c:a libmp3lame"
        );
    }

    #[test]
    fn codec_and_strip_audio_shape_encoder_args() {
        let hevc = Options {
            video_codec: Some(VideoCodec::Hevc),
            ..Options::default()
        };
        let mp4 = output_args("mp4", &hevc, &Budget::default()).join(" ");
        assert!(
            mp4.contains("-c:v libx265") && mp4.contains("-tag:v hvc1") && !mp4.contains("libx264"),
            "{mp4}"
        );
        assert!(
            !output_args("mkv", &hevc, &Budget::default())
                .join(" ")
                .contains("hvc1")
        );
        // The codec only applies where it can: WebM stays VP9.
        assert!(
            output_args("webm", &hevc, &Budget::default())
                .join(" ")
                .contains("libvpx-vp9")
        );

        let silent = Options {
            strip_audio: true,
            ..Options::default()
        };
        for to in ["mp4", "mov", "mkv", "webm", "avi"] {
            let args = output_args(to, &silent, &Budget::default());
            assert!(args.contains(&"-an".to_string()), "{to}: {args:?}");
            assert!(!args.contains(&"-c:a".to_string()), "{to}: {args:?}");
        }
        // Audio targets ignore it rather than producing an empty file.
        assert!(!output_args("mp3", &silent, &Budget::default()).contains(&"-an".to_string()));
    }

    #[test]
    fn same_format_steps_are_explicit() {
        let steps = FfmpegEngine::new().steps();
        let mut same: Vec<_> = steps
            .iter()
            .filter(|s| s.from == s.to)
            .map(|s| s.to.id)
            .collect();
        same.sort_unstable();
        let mut expected = SAME.to_vec();
        expected.sort_unstable();
        assert_eq!(same, expected);
    }

    #[test]
    fn strip_metadata_and_fps_shape_encoder_args() {
        let strip = Options {
            strip_metadata: true,
            ..Options::default()
        };
        for to in ["mp4", "webm", "gif", "png", "mp3", "flac"] {
            let args = output_args(to, &strip, &Budget::default()).join(" ");
            assert!(
                args.starts_with("-map_metadata -1 -map_chapters -1 "),
                "{to}: {args}"
            );
        }
        let fps = Options {
            fps: Some(24),
            video_height: Some(480),
            ..Options::default()
        };
        let mp4 = output_args("mp4", &fps, &Budget::default());
        assert_eq!(mp4.iter().filter(|a| *a == "-vf").count(), 1, "{mp4:?}");
        assert!(
            mp4.join(" ")
                .ends_with(&format!("-vf fps=24,{}", video_scale(480))),
            "{mp4:?}"
        );
        let gif = output_args("gif", &fps, &Budget::default()).join(" ");
        assert!(gif.contains("fps=24,") && !gif.contains("fps=12"), "{gif}");
        for to in ["mp3", "png"] {
            let args = output_args(to, &fps, &Budget::default()).join(" ");
            assert!(!args.contains("fps"), "{to}: {args}");
        }
    }

    #[test]
    fn a_budget_replaces_quality_with_bitrates() {
        let o = Options {
            quality: Some(90),
            ..Options::default()
        };
        let plan = Budget {
            video_kbps: Some(400),
            audio_kbps: Some(64),
            gif_scale: None,
        };
        for to in ["mp4", "webm", "avi"] {
            let args = output_args(to, &o, &plan).join(" ");
            assert!(
                args.contains("-b:v 400k -maxrate 400k -bufsize 800k")
                    && args.contains("-b:a 64k")
                    && !args.contains("-crf")
                    && !args.contains("-q:v")
                    && !args.contains("-b:v 0"),
                "{to}: {args}"
            );
        }
        let audio = Budget {
            audio_kbps: Some(48),
            ..Budget::default()
        };
        let mp3 = output_args("mp3", &o, &audio).join(" ");
        assert!(mp3.contains("-b:a 48k") && !mp3.contains("-q:a"), "{mp3}");
        let smaller = Budget {
            gif_scale: Some(0.5),
            ..Budget::default()
        };
        let gif = output_args("gif", &o, &smaller).join(" ");
        assert!(
            gif.contains(",scale='max(16,trunc(iw*0.5000/2)*2)'"),
            "{gif}"
        );
    }

    #[test]
    fn budget_splits_or_refuses_a_target() {
        let o = Options::default();
        // 1MB over 10 s: 760 kbit/s, 95 of them audio.
        let video = budget("mp4", &o, 760.0, 1_000_000, 10.0).unwrap();
        assert_eq!((video.video_kbps, video.audio_kbps), (Some(665), Some(95)));
        let silent = Options {
            strip_audio: true,
            ..Options::default()
        };
        let video = budget("mp4", &silent, 760.0, 1_000_000, 10.0).unwrap();
        assert_eq!((video.video_kbps, video.audio_kbps), (Some(760), None));
        let err = budget("mp4", &o, 60.0, 10_000, 1.3)
            .unwrap_err()
            .to_string();
        assert!(
            err.contains("10KB is too small for 1.3 s of video"),
            "{err}"
        );
        // Audio stays within what each encoder accepts.
        assert_eq!(
            budget("opus", &o, 900.0, 1, 1.0).unwrap().audio_kbps,
            Some(256)
        );
        assert_eq!(
            budget("mp3", &o, 64.5, 1, 1.0).unwrap().audio_kbps,
            Some(64)
        );
        assert!(budget("ogg", &o, 40.0, 10_000, 2.0).is_err());
        let err = budget("flac", &o, 900.0, 1, 1.0).unwrap_err().to_string();
        assert!(err.contains("FLAC") && err.contains("MP3 or M4A"), "{err}");
        assert!(budget("wav", &o, 900.0, 1, 1.0).is_err());
        assert_eq!(budget("gif", &o, 1.0, 1, 1.0).unwrap(), Budget::default());
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

    /// The engine, or `None` (after saying so) when FFmpeg or ffprobe is
    /// missing.
    fn engine_with_probe() -> Option<FfmpegEngine> {
        let engine = FfmpegEngine::new();
        if engine.ffmpeg.is_none() || engine.ffprobe.is_none() {
            eprintln!("skipping: FFmpeg or ffprobe not found");
            return None;
        }
        Some(engine)
    }

    /// Generates an input with FFmpeg from `args` (inputs and encoders).
    fn generate(engine: &FfmpegEngine, path: &Path, args: &[&str]) {
        let out = Command::new(engine.ffmpeg.as_ref().unwrap())
            .args(["-v", "error", "-y"])
            .args(args)
            .arg(path)
            .output()
            .unwrap();
        assert!(out.status.success(), "generate {path:?}: {out:?}");
    }

    /// Runs one step and returns the output and the last progress reported.
    fn run_step(
        engine: &FfmpegEngine,
        input: &Path,
        to: &str,
        options: &Options,
    ) -> Result<(PathBuf, f32)> {
        let step = Step {
            from: convt_core::format_by_extension(input).unwrap(),
            to: convt_core::format_by_id(to).unwrap(),
        };
        let last = std::sync::Mutex::new(0.0f32);
        let progress = |p: Option<f32>| {
            if let Some(p) = p {
                *last.lock().unwrap() = p;
            }
        };
        let cancel = convt_core::Cancel::new();
        let ctx = Ctx::new(step, options, &progress, &cancel);
        let out = input.with_extension(format!("{to}.out"));
        std::fs::create_dir_all(&out).unwrap();
        let outputs = engine.convert(&ctx, input, &out)?;
        let last = *last.lock().unwrap();
        Ok((outputs[0].clone(), last))
    }

    /// One `-show_entries` value from ffprobe, as `key=value` lines.
    fn probe(engine: &FfmpegEngine, path: &Path, entries: &str) -> String {
        let out = Command::new(engine.ffprobe.as_ref().unwrap())
            .args([
                "-v",
                "error",
                "-show_entries",
                entries,
                "-of",
                "default=nw=1",
            ])
            .arg(path)
            .output()
            .unwrap();
        assert!(out.status.success(), "ffprobe {path:?}: {out:?}");
        String::from_utf8(out.stdout).unwrap().to_lowercase()
    }

    fn probe_duration(engine: &FfmpegEngine, path: &Path) -> f64 {
        probe(engine, path, "format=duration")
            .trim()
            .trim_start_matches("duration=")
            .parse()
            .unwrap()
    }

    /// A 320x240 clip with noise and a tone: hard to compress, so a size
    /// target has to work for it.
    fn noisy_clip(engine: &FfmpegEngine, path: &Path, secs: u32) {
        let video = format!("testsrc2=size=320x240:rate=25:duration={secs},noise=alls=40:allf=t+u");
        let audio = format!("anoisesrc=duration={secs}:amplitude=0.3");
        generate(
            engine,
            path,
            &[
                "-f",
                "lavfi",
                "-i",
                &video,
                "-f",
                "lavfi",
                "-i",
                &audio,
                "-c:v",
                "libx264",
                "-crf",
                "18",
                "-pix_fmt",
                "yuv420p",
                "-c:a",
                "aac",
                "-shortest",
            ],
        );
    }

    #[test]
    fn real_ffmpeg_strips_metadata() {
        let Some(engine) = engine_with_probe() else {
            return;
        };
        let dir = tempfile::tempdir().unwrap();
        for (from, to) in [
            ("mp4", "mp4"),
            ("mov", "mkv"),
            ("mp3", "mp3"),
            ("mp3", "ogg"),
        ] {
            let input = dir.path().join(format!("tagged-{to}.{from}"));
            let video: &[&str] = if from == "mp3" {
                &[]
            } else {
                &["-f", "lavfi", "-i", "testsrc=size=64x48:rate=10:duration=1"]
            };
            let args = [
                video,
                &[
                    "-f",
                    "lavfi",
                    "-i",
                    "sine=duration=1",
                    "-shortest",
                    "-metadata",
                    "title=secret",
                    "-metadata",
                    "location=+40.0-074.0",
                ],
            ]
            .concat();
            generate(&engine, &input, &args);
            let (kept, _) = run_step(&engine, &input, to, &Options::default()).unwrap();
            let tags = probe(&engine, &kept, "format_tags:stream_tags");
            assert!(tags.contains("secret"), "{from}->{to} copies tags: {tags}");
            let strip = Options {
                strip_metadata: true,
                ..Options::default()
            };
            let (stripped, _) = run_step(&engine, &input, to, &strip).unwrap();
            let tags = probe(&engine, &stripped, "format_tags:stream_tags");
            assert!(
                !tags.contains("secret") && !tags.contains("+40.0"),
                "{from}->{to}: {tags}"
            );
        }
    }

    #[test]
    fn real_ffmpeg_trims_and_reports_trimmed_progress() {
        let Some(engine) = engine_with_probe() else {
            return;
        };
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("three.mp4");
        generate(
            &engine,
            &input,
            &[
                "-f",
                "lavfi",
                "-i",
                "testsrc=size=64x48:rate=25:duration=3",
                "-f",
                "lavfi",
                "-i",
                "sine=duration=3",
                "-c:v",
                "libx264",
                "-pix_fmt",
                "yuv420p",
                "-shortest",
            ],
        );
        let at = |s: &str| Some(s.parse::<Timestamp>().unwrap());
        let trim = Options {
            trim_start: at("1"),
            trim_end: at("2"),
            ..Options::default()
        };
        for to in ["mp4", "webm", "gif", "mp3"] {
            let (output, progress) = run_step(&engine, &input, to, &trim).unwrap();
            let secs = probe_duration(&engine, &output);
            assert!((secs - 1.0).abs() <= 0.15, "{to}: {secs} s");
            assert!(progress > 0.9, "{to}: progress ended at {progress}");
        }
        // A trim end past the input keeps the rest of it.
        let tail = Options {
            trim_start: at("2.5"),
            trim_end: at("10"),
            ..Options::default()
        };
        let (output, _) = run_step(&engine, &input, "mp4", &tail).unwrap();
        assert!((probe_duration(&engine, &output) - 0.5).abs() <= 0.15);
        let late = Options {
            trim_start: at("3.5"),
            ..Options::default()
        };
        let err = run_step(&engine, &input, "mp4", &late).unwrap_err();
        assert!(
            matches!(err, Error::InvalidOption(_)) && err.to_string().contains("past the end"),
            "{err}"
        );
        // A still frame comes from the trim start.
        let (frame, _) = run_step(&engine, &input, "png", &trim).unwrap();
        assert!(image::open(frame).is_ok());
    }

    #[test]
    fn real_ffmpeg_applies_fps() {
        let Some(engine) = engine_with_probe() else {
            return;
        };
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("clip.mp4");
        generate(
            &engine,
            &input,
            &[
                "-f",
                "lavfi",
                "-i",
                "testsrc=size=64x48:rate=25:duration=1",
                "-c:v",
                "libx264",
                "-pix_fmt",
                "yuv420p",
            ],
        );
        let fps = Options {
            fps: Some(5),
            video_height: Some(24),
            ..Options::default()
        };
        for to in ["mp4", "gif"] {
            let (output, _) = run_step(&engine, &input, to, &fps).unwrap();
            let stream = probe(&engine, &output, "stream=avg_frame_rate,height");
            assert!(
                stream.contains("avg_frame_rate=5/1") && stream.contains("height=24"),
                "{to}: {stream}"
            );
        }
    }

    #[test]
    fn real_ffmpeg_meets_a_target_size() {
        let Some(engine) = engine_with_probe() else {
            return;
        };
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("noisy.mp4");
        noisy_clip(&engine, &input, 4);
        let original = std::fs::metadata(&input).unwrap().len();
        let sized = |bytes| Options {
            target_size: Some(bytes),
            quality: Some(100),
            ..Options::default()
        };
        for (to, target) in [("mp4", 100_000), ("webm", 100_000), ("gif", 150_000)] {
            assert!(original > target, "the clip is already small: {original}");
            let (output, progress) = run_step(&engine, &input, to, &sized(target))
                .unwrap_or_else(|e| panic!("{to}: {e}"));
            let size = std::fs::metadata(&output).unwrap().len();
            eprintln!("{to}: {original} -> {size} bytes (target {target})");
            assert!(size <= target, "{to}: {size} > {target}");
            // The output still plays for the full length.
            assert!(
                (probe_duration(&engine, &output) - 4.0).abs() <= 0.3,
                "{to}"
            );
            assert!(progress > 0.85, "{to}: progress ended at {progress}");
        }
        let wav = dir.path().join("noise.wav");
        generate(
            &engine,
            &wav,
            &["-f", "lavfi", "-i", "anoisesrc=duration=10", "-ac", "2"],
        );
        let (mp3, _) = run_step(&engine, &wav, "mp3", &sized(60_000)).unwrap();
        let size = std::fs::metadata(&mp3).unwrap().len();
        assert!(size <= 60_000, "mp3: {size}");
        assert!((probe_duration(&engine, &mp3) - 10.0).abs() <= 0.3);
        let err = run_step(&engine, &wav, "flac", &sized(60_000)).unwrap_err();
        assert!(err.to_string().contains("MP3 or M4A"), "{err}");
        let err = run_step(&engine, &input, "mp4", &sized(10_000)).unwrap_err();
        assert!(
            matches!(err, Error::InvalidOption(_)) && err.to_string().contains("too small"),
            "{err}"
        );
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
