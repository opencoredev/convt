use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use convt_core::{Background, Ctx, Engine, Error, Options, Result, Step, Timestamp, VideoCodec};

const VIDEO_IN: &[&str] = &["mp4", "mov", "webm", "mkv", "avi", "gif"];
const VIDEO_OUT: &[&str] = &["mp4", "mov", "webm", "mkv", "avi", "gif"];
/// Video to image grabs the first frame. WebP goes through PNG and the
/// `image` engine: FFmpeg has no WebP encoder of its own, and the bundled
/// builds leave out libwebp.
const FRAME: &[&str] = &["png", "jpeg"];
const AUDIO: &[&str] = &[
    "mp3", "wav", "flac", "aac", "m4a", "ogg", "opus", "aiff", "caf", "m4r",
];
/// Containers whose first subtitle stream can be extracted as text. MP4 and
/// MOV carry mov_text, MKV and WebM carry SubRip, ASS or WebVTT.
const SUBTITLE_IN: &[&str] = &["mp4", "mov", "mkv", "webm"];
const SUBTITLE_OUT: &[&str] = &["srt", "vtt"];
/// Formats re-encoded into themselves, to shrink, trim, strip or retime a
/// file. GIF is here so its animation survives; the image engine would keep
/// only the first frame. CAF (Apple Lossless) is here for trimming and
/// stripping, like FLAC; AIFF stays out, like WAV.
const SAME: &[&str] = &[
    "mp4", "mov", "webm", "mkv", "gif", "mp3", "m4a", "m4r", "ogg", "opus", "flac", "caf",
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
    "mov,mp4,m4a,3gp,3g2,mj2,matroska,webm,avi,gif,mp3,wav,flac,aac,ogg,aiff,caf",
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
        Ok(self
            .format_secs(ctx, input, "duration")?
            .map(|s| s * 1_000_000.0))
    }

    /// One numeric `format` field from ffprobe, in seconds, or `None` when
    /// ffprobe is missing or can't tell. Only cancellation is an error.
    fn format_secs(&self, ctx: &Ctx, input: &Path, field: &str) -> Result<Option<f64>> {
        let Some(ffprobe) = &self.ffprobe else {
            return Ok(None);
        };
        let mut cmd = Command::new(ffprobe);
        cmd.args(LOCAL_INPUT_ARGS)
            .args(["-v", "error", "-show_entries"])
            .arg(format!("format={field}"))
            .args(["-of", "csv=p=0"])
            .arg(local_path(input));
        let mut secs = None;
        match crate::run_tool("ffprobe", cmd, ctx, |line| {
            secs = secs.or_else(|| line.trim().parse::<f64>().ok());
        }) {
            Err(Error::Cancelled) => Err(Error::Cancelled),
            Err(_) => Ok(None),
            Ok(()) => Ok(secs),
        }
    }

    /// How many audio streams the input has. Without ffprobe, or when it
    /// can't tell, assume one, which only costs a target size some video
    /// bitrate.
    fn audio_tracks(&self, ctx: &Ctx, input: &Path) -> Result<u32> {
        let Some(ffprobe) = &self.ffprobe else {
            return Ok(1);
        };
        let mut cmd = Command::new(ffprobe);
        cmd.args(LOCAL_INPUT_ARGS)
            .args([
                "-v",
                "error",
                "-select_streams",
                "a",
                "-show_entries",
                "stream=index",
                "-of",
                "csv=p=0",
            ])
            .arg(local_path(input));
        let mut found = 0;
        match crate::run_tool("ffprobe", cmd, ctx, |line| {
            found += u32::from(!line.trim().is_empty());
        }) {
            Err(Error::Cancelled) => Err(Error::Cancelled),
            Err(_) => Ok(1),
            Ok(()) => Ok(found),
        }
    }
}

/// Targets are offered per format, so a silent video still lists audio ones
/// and a video without subtitles still lists SRT. Name the cause instead of
/// passing on FFmpeg's empty-output or stream-map message.
fn missing_stream_error(e: Error, to: &str) -> Error {
    let message = match &e {
        Error::EngineFailed { message, .. }
            if AUDIO.contains(&to) && message.contains("does not contain any stream") =>
        {
            "this file has no audio track to convert"
        }
        Error::EngineFailed { message, .. }
            if SUBTITLE_OUT.contains(&to) && message.contains("matches no streams") =>
        {
            "this video has no subtitle track"
        }
        // DVD and Blu-ray subtitles are pictures; FFmpeg can't OCR them.
        Error::EngineFailed { message, .. }
            if SUBTITLE_OUT.contains(&to)
                && message.contains("text to text or bitmap to bitmap") =>
        {
            "this video's subtitles are images, not text, so they can't be saved as text"
        }
        _ => return e,
    };
    Error::EngineFailed {
        engine: "ffmpeg",
        message: message.into(),
    }
}

/// A hardware video encoder family. VideoToolbox (macOS) and NVENC (NVIDIA
/// on Windows and Linux) take frames from system memory, so the software
/// path's filters and pixel format work unchanged. VAAPI and Quick Sync are
/// left out: they need a device chosen per machine and `hwupload` or `nv12`
/// conversions that change the filter chain, and none of the release
/// platforms needs them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum HwEncoder {
    VideoToolbox,
    Nvenc,
}

impl HwEncoder {
    /// The families worth probing on this OS, in preference order.
    fn candidates() -> &'static [HwEncoder] {
        if cfg!(target_os = "macos") {
            &[HwEncoder::VideoToolbox]
        } else if cfg!(any(windows, target_os = "linux")) {
            &[HwEncoder::Nvenc]
        } else {
            &[]
        }
    }

    fn encoder(self, codec: VideoCodec) -> &'static str {
        match (self, codec) {
            (HwEncoder::VideoToolbox, VideoCodec::H264) => "h264_videotoolbox",
            (HwEncoder::VideoToolbox, VideoCodec::Hevc) => "hevc_videotoolbox",
            (HwEncoder::Nvenc, VideoCodec::H264) => "h264_nvenc",
            (HwEncoder::Nvenc, VideoCodec::Hevc) => "hevc_nvenc",
        }
    }

    /// Encoder and rate-control arguments for 1-100 `quality`, `None`
    /// meaning the software default's equivalent (quality 80, CRF 20). A
    /// `kbps` from a target size's budget replaces quality with a bitrate,
    /// as it does for the software encoders.
    fn video_args(self, codec: VideoCodec, quality: Option<u8>, kbps: Option<u32>) -> Vec<String> {
        let q = f32::from(quality.unwrap_or(80));
        let mut args = vec!["-c:v".to_string(), self.encoder(codec).to_string()];
        match (self, kbps) {
            // VideoToolbox has no -maxrate on every macOS, so its average
            // is the whole cap; the retry loop catches an overshoot.
            (HwEncoder::VideoToolbox, Some(v)) => {
                args.extend(["-b:v".into(), format!("{v}k")]);
            }
            (HwEncoder::Nvenc, Some(v)) => {
                let (v, buf) = (format!("{v}k"), format!("{}k", v * 2));
                args.extend(
                    [
                        "-preset", "p5", "-tune", "hq", "-rc", "vbr", "-b:v", &v, "-maxrate", &v,
                        "-bufsize", &buf,
                    ]
                    .map(String::from),
                );
            }
            // Constant quality, where higher is better. Intel Macs without
            // a constant-quality encoder fail the probe and stay software.
            (HwEncoder::VideoToolbox, None) => {
                args.extend(["-q:v".into(), format!("{:.0}", 20.0 + q * 0.6)]);
            }
            // Constant quality with VBR, on a scale like CRF. NVENC files run
            // a little larger than x264's at the same number, so it starts two
            // points higher.
            (HwEncoder::Nvenc, None) => {
                let worst = match codec {
                    VideoCodec::H264 => 42.0,
                    VideoCodec::Hevc => 46.0,
                };
                let cq = crf(quality.unwrap_or(80), worst);
                args.extend(
                    [
                        "-preset", "p5", "-tune", "hq", "-rc", "vbr", "-cq", &cq, "-b:v", "0",
                    ]
                    .map(String::from),
                );
            }
        }
        args
    }
}

/// Encoder names from `ffmpeg -encoders`, which lists one per line after a
/// dashed rule as `<flags> <name> <description>`.
fn listed_encoders(list: &str) -> Vec<&str> {
    list.lines()
        .skip_while(|line| !line.trim_start().starts_with("---"))
        .skip(1)
        .filter_map(|line| {
            let mut words = line.split_whitespace();
            let flags = words.next()?;
            let name = words.next()?;
            (flags.len() == 6 && flags.starts_with('V')).then_some(name)
        })
        .collect()
}

/// Runs `run` with `hw`, and once more in software when that fails for any
/// reason but cancellation, calling `before_retry` first to clear the failed
/// attempt's output.
fn with_software_fallback(
    hw: Option<HwEncoder>,
    mut run: impl FnMut(Option<HwEncoder>) -> Result<()>,
    before_retry: impl FnOnce(),
) -> Result<()> {
    let Some(hw) = hw else {
        return run(None);
    };
    match run(Some(hw)) {
        Err(e) if !matches!(e, Error::Cancelled) => {
            before_retry();
            run(None)
        }
        done => done,
    }
}

/// Hardware encoding is on unless `CONVT_HWACCEL` is `0`, `off`, `false` or
/// `no`, and never inside the cloud worker's sandbox, where there is no GPU
/// and device access is refused.
fn hardware_allowed() -> bool {
    if crate::CLOUD_SUPERVISED.load(std::sync::atomic::Ordering::SeqCst) {
        return false;
    }
    !std::env::var("CONVT_HWACCEL").is_ok_and(|v| hwaccel_off(&v))
}

fn hwaccel_off(value: &str) -> bool {
    matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "0" | "off" | "false" | "no"
    )
}

/// How long a probe encode may take before the encoder counts as broken.
const PROBE_TIMEOUT: Duration = Duration::from_secs(15);

/// The working hardware encoder per FFmpeg binary and codec, probed once per
/// process. Being listed proves nothing (FFmpeg lists NVENC on machines
/// without an NVIDIA GPU), so each candidate must encode a few real frames.
static HARDWARE: OnceLock<Mutex<HardwareCache>> = OnceLock::new();
type HardwareCache = HashMap<(PathBuf, VideoCodec), Option<HwEncoder>>;

fn hardware_encoder(ffmpeg: &Path, codec: VideoCodec, ctx: &Ctx) -> Result<Option<HwEncoder>> {
    let cache = HARDWARE.get_or_init(Mutex::default);
    // Held through the probe, so concurrent jobs wait for one answer.
    let mut cache = cache.lock().unwrap_or_else(|e| e.into_inner());
    let key = (ffmpeg.to_path_buf(), codec);
    if let Some(found) = cache.get(&key) {
        return Ok(*found);
    }
    let found = probe_hardware(ffmpeg, codec, ctx)?;
    cache.insert(key, found);
    Ok(found)
}

fn probe_hardware(ffmpeg: &Path, codec: VideoCodec, ctx: &Ctx) -> Result<Option<HwEncoder>> {
    let mut list = Command::new(ffmpeg);
    list.args(["-hide_banner", "-encoders"]);
    let Some(list) = run_probe(list, ctx)? else {
        return Ok(None);
    };
    let listed = listed_encoders(&list);
    for &hw in HwEncoder::candidates() {
        if !listed.contains(&hw.encoder(codec)) {
            continue;
        }
        let mut test = Command::new(ffmpeg);
        test.args(["-hide_banner", "-nostdin", "-v", "error"])
            .args(["-f", "lavfi", "-i", "color=c=gray:s=256x256:r=10:d=0.3"])
            .args(hw.video_args(codec, None, None))
            .args(["-pix_fmt", "yuv420p", "-f", "null", "-"]);
        if run_probe(test, ctx)?.is_some() {
            return Ok(Some(hw));
        }
    }
    Ok(None)
}

/// Runs a probe to completion and returns its stdout, or `None` when it
/// fails or outlives [`PROBE_TIMEOUT`]. Only cancellation is an error.
fn run_probe(mut cmd: Command, ctx: &Ctx) -> Result<Option<String>> {
    crate::hide_console(&mut cmd);
    let Ok(mut child) = cmd
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
    else {
        return Ok(None);
    };
    // `-encoders` prints a few kilobytes, more than a pipe buffer on some
    // systems, so read while waiting.
    let mut stdout = child.stdout.take().expect("piped");
    let reader = std::thread::spawn(move || {
        let mut out = String::new();
        let _ = std::io::Read::read_to_string(&mut stdout, &mut out);
        out
    });
    let deadline = Instant::now() + PROBE_TIMEOUT;
    let status = loop {
        if ctx.is_cancelled() || Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            if ctx.is_cancelled() {
                return Err(Error::Cancelled);
            }
            return Ok(None);
        }
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => std::thread::sleep(Duration::from_millis(20)),
            Err(_) => return Ok(None),
        }
    };
    let out = reader.join().unwrap_or_default();
    Ok(status.success().then_some(out))
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
        "aac" | "m4a" | "m4r" => Some((8.0, 320.0)),
        "ogg" => Some((48.0, 320.0)),
        "opus" => Some((6.0, 256.0)),
        _ => None,
    }
}

/// The highest video bitrate a budget asks for, in kbit/s. Far above what
/// any encoder here produces, and small enough that doubling it for the
/// buffer size can't overflow.
const MAX_VIDEO_KBPS: f64 = 1_000_000.0;

/// Splits `total_kbps` between video and audio for `to`, or says why a
/// `target` of bytes can't hold `secs` of it. `tracks` is how many audio
/// tracks the output carries, each at the same bitrate; a silent input or
/// `strip_audio` reserves none.
fn budget(
    to: &str,
    o: &Options,
    tracks: u32,
    total_kbps: f64,
    target: u64,
    secs: f64,
) -> Result<Budget> {
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
        let audio = if o.strip_audio || tracks == 0 {
            None
        } else {
            Some(
                o.audio_bitrate
                    .unwrap_or((total_kbps / 8.0 / f64::from(tracks)).clamp(32.0, 128.0) as u32),
            )
        };
        let video = total_kbps - f64::from(audio.unwrap_or(0)) * f64::from(tracks);
        if video < 50.0 {
            return Err(too_small("video"));
        }
        return Ok(Budget {
            video_kbps: Some(video.min(MAX_VIDEO_KBPS) as u32),
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

/// [`encoder_args`] with the software encoders.
#[cfg(test)]
fn output_args(to: &str, o: &Options, budget: &Budget) -> Vec<String> {
    encoder_args(to, o, budget, None)
}

/// Encoder arguments for one output format and the user's options. A
/// `budget` from `target_size` replaces CRF and quality scales with
/// bitrates, so an explicit `quality` is ignored then. `hw` replaces libx264
/// or libx265 for MP4, MOV and MKV; everything else (pixel format, tags,
/// audio, faststart, filters) is the same on both paths.
fn encoder_args(to: &str, o: &Options, budget: &Budget, hw: Option<HwEncoder>) -> Vec<String> {
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
            let codec = o.video_codec.unwrap_or(VideoCodec::H264);
            let (encoder, crf) = match codec {
                VideoCodec::H264 => ("libx264", x264_crf),
                VideoCodec::Hevc => ("libx265", q.map_or("24".into(), |q| crf(q, 44.0))),
            };
            match (hw, &rate) {
                (Some(hw), _) => {
                    let video = hw.video_args(codec, q, budget.video_kbps);
                    push(&video.iter().map(String::as_str).collect::<Vec<_>>());
                }
                (None, Some((v, buf))) => push(&[
                    "-c:v", encoder, "-preset", "medium", "-b:v", v, "-maxrate", v, "-bufsize", buf,
                ]),
                (None, None) => push(&["-c:v", encoder, "-preset", "medium", "-crf", &crf]),
            }
            // QuickTime and Apple devices only play HEVC tagged hvc1.
            if codec == VideoCodec::Hevc && to != "mkv" {
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
        // An iPhone ringtone is an M4A by another name. FFmpeg doesn't know
        // the extension, so name the muxer.
        "m4r" => push(&["-vn", "-c:a", "aac", "-b:a", &aac, "-f", "ipod"]),
        // Big-endian PCM, the AIFF Apple tools write.
        "aiff" => push(&["-vn", "-c:a", "pcm_s16be"]),
        // Apple Lossless, which every Apple app reads from CAF, at a
        // fraction of PCM's size.
        "caf" => push(&["-vn", "-c:a", "alac"]),
        "srt" => push(&["-map", "0:s:0", "-c:s", "srt", "-f", "srt"]),
        "vtt" => push(&["-map", "0:s:0", "-c:s", "webvtt", "-f", "webvtt"]),
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
            .chain(crate::steps(SUBTITLE_IN, SUBTITLE_OUT))
            .chain(same)
            .collect()
    }

    fn convert(&self, ctx: &Ctx, input: &Path, out_dir: &Path) -> Result<Vec<PathBuf>> {
        let ffmpeg = self.ffmpeg.as_ref().ok_or(Error::EngineUnavailable {
            engine: "ffmpeg",
            reason: "ffmpeg not found".into(),
        })?;
        let hw = if matches!(ctx.step.to.id, "mp4" | "mov" | "mkv") && hardware_allowed() {
            let codec = ctx.options.video_codec.unwrap_or(VideoCodec::H264);
            hardware_encoder(ffmpeg, codec, ctx)?
        } else {
            None
        };
        self.convert_with(ctx, input, out_dir, hw)
    }
}

impl FfmpegEngine {
    /// Converts with `hw` when given. An encode that fails in hardware for
    /// any reason but cancellation deletes its output and runs again in
    /// software, and later attempts stay in software. A probe can pass while
    /// a particular input still fails in hardware (too large a frame, a busy
    /// or reset GPU).
    fn convert_with(
        &self,
        ctx: &Ctx,
        input: &Path,
        out_dir: &Path,
        hw: Option<HwEncoder>,
    ) -> Result<Vec<PathBuf>> {
        let ffmpeg = self.ffmpeg.as_ref().ok_or(Error::EngineUnavailable {
            engine: "ffmpeg",
            reason: "ffmpeg not found".into(),
        })?;
        let o = ctx.options;
        let to = ctx.step.to.id;
        let subtitle = SUBTITLE_OUT.contains(&to);
        // FFmpeg's seeks keep or drop a cue that starts before the trim
        // instead of moving the rest onto the trimmed clock, so the times
        // would be wrong.
        if subtitle && (o.trim_start.is_some() || o.trim_end.is_some()) {
            return Err(Error::InvalidOption(
                "trimming applies to video and audio, not subtitles; extract them whole".into(),
            ));
        }
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
        // Still frames leave target_size to the image engine's encoder, and
        // subtitle text is too small for one to matter.
        let target = o.target_size.filter(|_| !still && !subtitle);
        let mut total_kbps = 0.0;
        // Re-encoding a video into its own container keeps every audio
        // track (several languages, commentary); other conversions keep
        // FFmpeg's pick of one.
        let keep_tracks = ctx.step.from == ctx.step.to && VIDEO_OUT.contains(&to) && to != "gif";
        let mut audio = 0;
        let mut plan = Budget::default();
        let target = match target {
            Some(target) => {
                let secs = secs.ok_or_else(|| {
                    Error::InvalidOption("a target size needs ffprobe to read the duration".into())
                })?;
                total_kbps = target as f64 * 8.0 * 0.95 / secs / 1000.0;
                audio = if VIDEO_OUT.contains(&to) && !o.strip_audio && to != "gif" {
                    let tracks = self.audio_tracks(ctx, input)?;
                    if keep_tracks { tracks } else { tracks.min(1) }
                } else {
                    0
                };
                plan = budget(to, o, audio, total_kbps, target, secs)?;
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
        // FFmpeg shifts output to start at the input's start time. AAC
        // priming often makes that negative in MKV (-0.023 s), which would
        // move every cue late; a later start (a trimmed recording) should
        // still be subtracted, since players count from it.
        let copy_times = subtitle
            && self
                .format_secs(ctx, input, "start_time")?
                .is_some_and(|start| start <= 0.0);
        let hw = std::cell::Cell::new(hw);
        let run = |budget: &Budget, hw: Option<HwEncoder>, from: f32, until: f32| {
            let mut cmd = Command::new(ffmpeg);
            cmd.args(["-hide_banner", "-nostdin", "-y", "-v", "error"])
                .args(copy_times.then_some("-copyts"))
                .args(LOCAL_INPUT_ARGS)
                .args(["-progress", "pipe:1", "-nostats"])
                .args(&seek)
                .arg("-i")
                .arg(local_path(input))
                .args(if keep_tracks {
                    // Subtitles are copied; MKV also keeps its attachments
                    // (the fonts styled subtitles use).
                    let mut maps = vec!["-map", "0:v:0?", "-map", "0:a?", "-map", "0:s?"];
                    if to == "mkv" {
                        maps.extend(["-map", "0:t?"]);
                    }
                    maps.extend(["-c:s", "copy"]);
                    maps
                } else {
                    Vec::new()
                })
                .args(encoder_args(
                    if via_png { "png" } else { to },
                    o,
                    budget,
                    hw,
                ))
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
        };
        let encode = |budget: &Budget, from: f32, until: f32| {
            with_software_fallback(
                hw.get(),
                |encoder| run(budget, encoder, from, until),
                || {
                    hw.set(None);
                    let _ = std::fs::remove_file(written);
                    if secs.is_some() {
                        ctx.progress(from);
                    }
                },
            )
            .map_err(|e| missing_stream_error(e, to))
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
                        plan = budget(to, o, audio, total_kbps, target, target_secs)?;
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
            let meta = crate::image::Meta {
                pixel_aspect,
                icc: None,
            };
            crate::image::encode_with_meta(img, to, ctx.options, &output, &meta, &|| ctx.check())?;
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
            missing_stream_error(empty(), "mp3")
                .to_string()
                .contains("no audio track")
        );
        assert!(
            missing_stream_error(empty(), "mp4")
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
        let video = budget("mp4", &o, 1, 760.0, 1_000_000, 10.0).unwrap();
        assert_eq!((video.video_kbps, video.audio_kbps), (Some(665), Some(95)));
        let silent = Options {
            strip_audio: true,
            ..Options::default()
        };
        let video = budget("mp4", &silent, 1, 760.0, 1_000_000, 10.0).unwrap();
        assert_eq!((video.video_kbps, video.audio_kbps), (Some(760), None));
        // An input without audio gets the whole budget for video too.
        // Two tracks reserve twice the audio.
        let two = budget("mp4", &o, 2, 760.0, 1_000_000, 10.0).unwrap();
        assert_eq!((two.video_kbps, two.audio_kbps), (Some(666), Some(47)));
        let video = budget("mp4", &o, 0, 760.0, 1_000_000, 10.0).unwrap();
        assert_eq!((video.video_kbps, video.audio_kbps), (Some(760), None));
        // A huge target stays a bitrate the buffer size can double.
        let huge = budget("mp4", &o, 0, 1e12, u64::MAX, 1.0).unwrap();
        assert_eq!(huge.video_kbps, Some(MAX_VIDEO_KBPS as u32));
        assert!(
            output_args("mp4", &o, &huge)
                .join(" ")
                .contains("-bufsize 2000000k")
        );
        let err = budget("mp4", &o, 1, 60.0, 10_000, 1.3)
            .unwrap_err()
            .to_string();
        assert!(
            err.contains("10KB is too small for 1.3 s of video"),
            "{err}"
        );
        // Audio stays within what each encoder accepts.
        assert_eq!(
            budget("opus", &o, 1, 900.0, 1, 1.0).unwrap().audio_kbps,
            Some(256)
        );
        assert_eq!(
            budget("mp3", &o, 1, 64.5, 1, 1.0).unwrap().audio_kbps,
            Some(64)
        );
        assert!(budget("ogg", &o, 1, 40.0, 10_000, 2.0).is_err());
        let err = budget("flac", &o, 1, 900.0, 1, 1.0)
            .unwrap_err()
            .to_string();
        assert!(err.contains("FLAC") && err.contains("MP3 or M4A"), "{err}");
        assert!(budget("wav", &o, 1, 900.0, 1, 1.0).is_err());
        assert_eq!(
            budget("gif", &o, 1, 1.0, 1, 1.0).unwrap(),
            Budget::default()
        );
    }

    #[test]
    fn apple_audio_and_subtitle_args() {
        let o = Options {
            audio_bitrate: Some(128),
            ..Options::default()
        };
        assert_eq!(
            output_args("aiff", &o, &Budget::default()).join(" "),
            "-vn -c:a pcm_s16be"
        );
        assert_eq!(
            output_args("caf", &o, &Budget::default()).join(" "),
            "-vn -c:a alac"
        );
        assert_eq!(
            output_args("m4r", &o, &Budget::default()).join(" "),
            "-vn -c:a aac -b:a 128k -f ipod"
        );
        assert_eq!(
            output_args("m4r", &Options::default(), &Budget::default()).join(" "),
            "-vn -c:a aac -b:a 192k -f ipod"
        );
        assert_eq!(
            output_args("srt", &o, &Budget::default()).join(" "),
            "-map 0:s:0 -c:s srt -f srt"
        );
        assert_eq!(
            output_args("vtt", &o, &Budget::default()).join(" "),
            "-map 0:s:0 -c:s webvtt -f webvtt"
        );
    }

    #[test]
    fn subtitles_only_come_out_of_video_containers() {
        let steps = FfmpegEngine::new().steps();
        let has = |from: &str, to: &str| steps.iter().any(|s| s.from.id == from && s.to.id == to);
        for from in ["mp4", "mov", "mkv", "webm"] {
            assert!(has(from, "srt") && has(from, "vtt"), "{from}");
        }
        for to in ["mp4", "mkv", "mp3", "srt", "vtt", "txt"] {
            assert!(!has("srt", to) && !has("vtt", to), "subtitles -> {to}");
        }
        assert!(!has("gif", "srt") && !has("avi", "srt") && !has("mp3", "srt"));
        for to in ["aiff", "caf", "m4r"] {
            assert!(has("mp4", to) && has("wav", to) && has(to, "mp3"), "{to}");
            assert!(!has("gif", to), "gif -> {to}");
        }
    }

    #[test]
    fn missing_subtitles_are_named() {
        let failed = |message: &str| Error::EngineFailed {
            engine: "ffmpeg",
            message: message.into(),
        };
        let map = "Stream map '0:s:0' matches no streams.";
        assert_eq!(
            missing_stream_error(failed(map), "srt").to_string(),
            "ffmpeg failed: this video has no subtitle track"
        );
        assert!(
            missing_stream_error(failed(map), "mp4")
                .to_string()
                .contains("matches no streams")
        );
        let bitmap =
            "Subtitle encoding currently only possible from text to text or bitmap to bitmap";
        assert!(
            missing_stream_error(failed(bitmap), "vtt")
                .to_string()
                .contains("images, not text")
        );
    }

    #[test]
    fn encoder_list_parsing() {
        let list = "Encoders:\n V..... = Video\n A..... = Audio\n ------\n \
            V....D libx264              libx264 H.264 (codec h264)\n \
            V....D h264_nvenc           NVIDIA NVENC H.264 encoder (codec h264)\n \
            V....D hevc_videotoolbox    VideoToolbox H.265 Encoder (codec hevc)\n \
            A....D aac                  AAC (Advanced Audio Coding)\n";
        assert_eq!(
            listed_encoders(list),
            ["libx264", "h264_nvenc", "hevc_videotoolbox"]
        );
        assert!(listed_encoders("garbage\nwithout a rule").is_empty());
    }

    #[test]
    fn hardware_encoder_args() {
        let hevc = Options {
            video_codec: Some(VideoCodec::Hevc),
            ..Options::default()
        };
        let best = Options {
            quality: Some(100),
            ..Options::default()
        };
        let args =
            |to: &str, o: &Options, hw| encoder_args(to, o, &Budget::default(), Some(hw)).join(" ");
        assert_eq!(
            args("mp4", &Options::default(), HwEncoder::VideoToolbox),
            "-c:v h264_videotoolbox -q:v 68 -pix_fmt yuv420p -c:a aac -b:a 192k -movflags +faststart"
        );
        assert_eq!(
            args("mov", &hevc, HwEncoder::VideoToolbox),
            "-c:v hevc_videotoolbox -q:v 68 -tag:v hvc1 -pix_fmt yuv420p -c:a aac -b:a 192k -movflags +faststart"
        );
        assert!(args("mp4", &best, HwEncoder::VideoToolbox).contains("-q:v 80"));
        assert_eq!(
            args("mkv", &Options::default(), HwEncoder::Nvenc),
            "-c:v h264_nvenc -preset p5 -tune hq -rc vbr -cq 22 -b:v 0 -pix_fmt yuv420p -c:a aac -b:a 192k"
        );
        let nvenc_hevc = args("mp4", &hevc, HwEncoder::Nvenc);
        assert!(
            nvenc_hevc.starts_with("-c:v hevc_nvenc ")
                && nvenc_hevc.contains("-cq 26")
                && nvenc_hevc.contains("-tag:v hvc1"),
            "{nvenc_hevc}"
        );
        assert!(!args("mkv", &hevc, HwEncoder::Nvenc).contains("hvc1"));
        assert!(args("mp4", &best, HwEncoder::Nvenc).contains("-cq 17"));
        // Options that aren't the encoder's carry over.
        let silent_small = Options {
            strip_audio: true,
            video_height: Some(480),
            ..Options::default()
        };
        let a = args("mp4", &silent_small, HwEncoder::Nvenc);
        assert!(
            a.contains("-an") && !a.contains("-c:a") && a.ends_with(&video_scale(480)),
            "{a}"
        );
        // Only H.264 and HEVC containers have hardware encoders.
        for to in ["webm", "avi", "gif", "mp3"] {
            assert_eq!(
                encoder_args(
                    to,
                    &Options::default(),
                    &Budget::default(),
                    Some(HwEncoder::Nvenc)
                ),
                output_args(to, &Options::default(), &Budget::default()),
                "{to}"
            );
        }
    }

    #[test]
    fn a_budget_puts_hardware_encoders_in_bitrate_mode() {
        let hevc = Options {
            video_codec: Some(VideoCodec::Hevc),
            quality: Some(90),
            ..Options::default()
        };
        let plan = Budget {
            video_kbps: Some(400),
            audio_kbps: Some(64),
            gif_scale: None,
        };
        let vt = encoder_args("mp4", &hevc, &plan, Some(HwEncoder::VideoToolbox)).join(" ");
        assert_eq!(
            vt,
            "-c:v hevc_videotoolbox -b:v 400k -tag:v hvc1 -pix_fmt yuv420p -c:a aac -b:a 64k -movflags +faststart"
        );
        let nvenc = encoder_args("mkv", &hevc, &plan, Some(HwEncoder::Nvenc)).join(" ");
        assert_eq!(
            nvenc,
            "-c:v hevc_nvenc -preset p5 -tune hq -rc vbr -b:v 400k -maxrate 400k -bufsize 800k -pix_fmt yuv420p -c:a aac -b:a 64k"
        );
        for args in [&vt, &nvenc] {
            assert!(!args.contains("-q:v") && !args.contains("-cq"), "{args}");
        }
    }

    #[test]
    fn hwaccel_escape_hatch() {
        for off in ["0", "off", "FALSE", " no "] {
            assert!(hwaccel_off(off), "{off}");
        }
        for on in ["1", "", "auto", "yes"] {
            assert!(!hwaccel_off(on), "{on}");
        }
    }

    #[test]
    fn hardware_failure_retries_in_software_but_cancellation_does_not() {
        let failed = || Error::EngineFailed {
            engine: "ffmpeg",
            message: "Error while opening encoder".into(),
        };
        let mut tries = Vec::new();
        let mut cleared = false;
        let result = with_software_fallback(
            Some(HwEncoder::Nvenc),
            |hw| {
                tries.push(hw);
                if hw.is_some() { Err(failed()) } else { Ok(()) }
            },
            || cleared = true,
        );
        assert!(result.is_ok() && cleared);
        assert_eq!(tries, [Some(HwEncoder::Nvenc), None]);

        let mut tries = Vec::new();
        let result = with_software_fallback(
            Some(HwEncoder::VideoToolbox),
            |hw| {
                tries.push(hw);
                Err(Error::Cancelled)
            },
            || panic!("cancellation must not retry"),
        );
        assert!(matches!(result, Err(Error::Cancelled)));
        assert_eq!(tries, [Some(HwEncoder::VideoToolbox)]);

        let mut tries = Vec::new();
        let result = with_software_fallback(
            None,
            |hw| {
                tries.push(hw);
                Err(failed())
            },
            || panic!("nothing to clear without hardware"),
        );
        assert!(result.is_err());
        assert_eq!(tries, [None]);

        // A software failure after a hardware one reports the software error.
        let result = with_software_fallback(
            Some(HwEncoder::Nvenc),
            |hw| {
                Err(Error::EngineFailed {
                    engine: "ffmpeg",
                    message: if hw.is_some() { "hw" } else { "sw" }.into(),
                })
            },
            || {},
        );
        assert_eq!(result.unwrap_err().to_string(), "ffmpeg failed: sw");
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
    fn compressing_a_video_keeps_every_audio_track() {
        let Some(engine) = engine_with_probe() else {
            return;
        };
        let dir = tempfile::tempdir().unwrap();
        for ext in ["mkv", "mp4"] {
            let input = dir.path().join(format!("two-tracks.{ext}"));
            generate(
                &engine,
                &input,
                &[
                    "-f",
                    "lavfi",
                    "-i",
                    "testsrc=size=64x48:rate=10:duration=2",
                    "-f",
                    "lavfi",
                    "-i",
                    "sine=frequency=440:duration=2",
                    "-f",
                    "lavfi",
                    "-i",
                    "sine=frequency=880:duration=2",
                    "-map",
                    "0",
                    "-map",
                    "1",
                    "-map",
                    "2",
                    "-c:v",
                    "libx264",
                    "-c:a",
                    "aac",
                ],
            );
            let tracks = |path: &Path| {
                probe(&engine, path, "stream=codec_type")
                    .lines()
                    .filter(|l| l.contains("audio"))
                    .count()
            };
            assert_eq!(tracks(&input), 2);
            let (same, _) = run_step(&engine, &input, ext, &Options::default()).unwrap();
            assert_eq!(tracks(&same), 2, "{ext} -> {ext} keeps both tracks");
            // Both count toward a target size.
            let sized = Options {
                target_size: Some(150_000),
                ..Options::default()
            };
            let (small, _) = run_step(&engine, &input, ext, &sized).unwrap();
            assert_eq!(tracks(&small), 2);
            assert!(std::fs::metadata(&small).unwrap().len() <= 150_000);
        }
    }

    #[test]
    fn compressing_a_video_keeps_its_subtitles() {
        let Some(engine) = engine_with_probe() else {
            return;
        };
        let dir = tempfile::tempdir().unwrap();
        let srt = dir.path().join("subs.srt");
        std::fs::write(&srt, "1\n00:00:00,000 --> 00:00:01,500\nHello\n").unwrap();
        for (ext, codec) in [("mkv", "srt"), ("mp4", "mov_text")] {
            let input = dir.path().join(format!("subbed.{ext}"));
            let mut args = vec![
                "-f",
                "lavfi",
                "-i",
                "testsrc=size=64x48:rate=10:duration=2",
                "-f",
                "lavfi",
                "-i",
                "sine=duration=2",
            ];
            let srt = srt.to_str().unwrap();
            args.extend(["-i", srt, "-map", "0", "-map", "1", "-map", "2"]);
            args.extend(["-c:v", "libx264", "-c:a", "aac", "-c:s", codec]);
            generate(&engine, &input, &args);
            let subs = |path: &Path| {
                probe(&engine, path, "stream=codec_type")
                    .lines()
                    .filter(|l| l.contains("subtitle"))
                    .count()
            };
            assert_eq!(subs(&input), 1, "{ext} fixture");
            let (same, _) = run_step(&engine, &input, ext, &Options::default()).unwrap();
            assert_eq!(subs(&same), 1, "{ext} -> {ext} keeps its subtitles");
        }
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
                    // Stream-scoped tags, which global stripping must reach.
                    "-metadata:s:a:0",
                    "title=streamsecret",
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
            if from != "mp3" {
                let video_tag = [
                    &["-f", "lavfi", "-i", "testsrc=size=64x48:rate=10:duration=1"][..],
                    &["-metadata:s:v:0", "title=videosecret"],
                ]
                .concat();
                let tagged = dir.path().join(format!("stream-{to}.{from}"));
                generate(&engine, &tagged, &video_tag);
                let (stripped, _) = run_step(&engine, &tagged, to, &strip).unwrap();
                let tags = probe(&engine, &stripped, "format_tags:stream_tags");
                assert!(!tags.contains("videosecret"), "{from}->{to}: {tags}");
            }
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

    /// Generates `name` in a fresh directory from FFmpeg arguments, or `None`
    /// when FFmpeg (or an encoder the fixture needs) is missing.
    fn fixture(name: &str, args: &[&str]) -> Option<(FfmpegEngine, tempfile::TempDir, PathBuf)> {
        let engine = FfmpegEngine::new();
        let Some(ffmpeg) = engine.ffmpeg.clone() else {
            eprintln!("SKIP: FFmpeg missing");
            return None;
        };
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(name);
        let out = Command::new(ffmpeg)
            .args(["-v", "error", "-y"])
            .args(args)
            .arg(&path)
            .output()
            .unwrap();
        if !out.status.success() {
            eprintln!(
                "SKIP: cannot generate {name}: {}",
                String::from_utf8_lossy(&out.stderr)
            );
            return None;
        }
        Some((engine, dir, path))
    }

    fn convert_file(
        engine: &FfmpegEngine,
        input: &Path,
        to: &str,
        options: &Options,
        hw: Option<HwEncoder>,
    ) -> (Result<Vec<PathBuf>>, PathBuf) {
        let from = convt_core::format_by_extension(input).unwrap();
        let step = Step {
            from,
            to: convt_core::format_by_id(to).unwrap(),
        };
        let cancel = convt_core::Cancel::new();
        let ctx = Ctx::new(step, options, &|_| {}, &cancel);
        let out = input.with_file_name(format!("out-{to}"));
        std::fs::create_dir_all(&out).unwrap();
        (engine.convert_with(&ctx, input, &out, hw), out)
    }

    const SUBTITLES: &str = "1\n00:00:00,000 --> 00:00:00,900\nHello from convt\n\n2\n00:00:01,000 --> 00:00:01,900\nSecond line\n";

    #[test]
    fn real_ffmpeg_extracts_subtitles() {
        let Some((engine, dir, _)) =
            fixture("probe.wav", &["-f", "lavfi", "-i", "anullsrc", "-t", "0.1"])
        else {
            return;
        };
        let srt = dir.path().join("subs.srt");
        std::fs::write(&srt, SUBTITLES).unwrap();
        let ffmpeg = engine.ffmpeg.clone().unwrap();
        // AAC priming gives the MKV a start time of -0.023 s, which must
        // not move the cues.
        let video = [
            "-f",
            "lavfi",
            "-i",
            "testsrc=size=64x48:rate=5:duration=2",
            "-f",
            "lavfi",
            "-i",
            "sine=duration=2",
        ];
        for (name, codec) in [
            ("subbed.mkv", "srt"),
            ("subbed.mp4", "mov_text"),
            ("plain.mp4", ""),
        ] {
            let path = dir.path().join(name);
            let mut cmd = Command::new(&ffmpeg);
            cmd.args(["-v", "error", "-y"]).args(video);
            if !codec.is_empty() {
                cmd.arg("-i")
                    .arg(&srt)
                    .args(["-map", "0", "-map", "1", "-map", "2", "-c:s", codec]);
            }
            let made = cmd
                .args(["-c:v", "mpeg4", "-c:a", "aac"])
                .arg(&path)
                .output()
                .unwrap();
            assert!(
                made.status.success(),
                "{}",
                String::from_utf8_lossy(&made.stderr)
            );
            for to in ["srt", "vtt"] {
                let (result, _) = convert_file(&engine, &path, to, &Options::default(), None);
                if codec.is_empty() {
                    let e = result.unwrap_err().to_string();
                    assert!(e.contains("this video has no subtitle track"), "{e}");
                    continue;
                }
                let text = std::fs::read_to_string(&result.unwrap()[0]).unwrap();
                assert!(
                    text.contains("Hello from convt") && text.contains("Second line"),
                    "{name} -> {to}: {text}"
                );
                if to == "vtt" {
                    assert!(
                        text.starts_with("WEBVTT") && text.contains("00:01.000 --> 00:01.900"),
                        "{text}"
                    );
                } else {
                    assert!(text.contains("00:00:01,000 --> 00:00:01,900"), "{text}");
                }
            }
            if codec.is_empty() {
                continue;
            }
            // FFmpeg can't trim cues onto the trimmed clock, so it's refused.
            let trimmed = Options {
                trim_start: Some(Timestamp { millis: 500 }),
                ..Options::default()
            };
            let (result, _) = convert_file(&engine, &path, "srt", &trimmed, None);
            assert!(
                matches!(result, Err(Error::InvalidOption(ref e)) if e.contains("not subtitles")),
                "{name} trimmed: {result:?}"
            );
        }
    }

    #[test]
    fn real_ffmpeg_writes_apple_audio() {
        let Some((engine, _dir, wav)) = fixture(
            "tone.wav",
            &["-f", "lavfi", "-i", "sine=frequency=440:duration=1"],
        ) else {
            return;
        };
        for (to, codec, container) in [
            ("aiff", "pcm_s16be", "aiff"),
            ("caf", "alac", "caf"),
            ("m4r", "aac", "mov,mp4,m4a,3gp,3g2,mj2"),
        ] {
            let (result, _) = convert_file(&engine, &wav, to, &Options::default(), None);
            let output = result.unwrap().remove(0);
            let Some(probe) = &engine.ffprobe else {
                eprintln!("SKIP stream check: ffprobe missing");
                return;
            };
            let info = Command::new(probe)
                .args([
                    "-v",
                    "error",
                    "-show_entries",
                    "stream=codec_name:format=format_name",
                ])
                .args(["-of", "default=nw=1:nk=1"])
                .arg(&output)
                .output()
                .unwrap();
            let info = String::from_utf8(info.stdout).unwrap();
            assert_eq!(info.lines().collect::<Vec<_>>(), [codec, container], "{to}");
            // And back: each one is also an input.
            let (back, _) = convert_file(&engine, &output, "wav", &Options::default(), None);
            assert!(back.is_ok(), "{to} -> wav: {back:?}");
        }
    }

    /// A forced hardware encoder that can't run here (VideoToolbox off macOS,
    /// NVENC without an NVIDIA GPU) must still produce the requested codec,
    /// in software, with nothing left over from the failed attempt. On a
    /// machine where the encoder works, this checks the hardware output.
    #[test]
    fn real_ffmpeg_falls_back_from_unusable_hardware() {
        let Some((engine, dir, clip)) = fixture(
            "clip.mov",
            &[
                "-f",
                "lavfi",
                "-i",
                "testsrc=size=128x96:rate=10:duration=1",
                "-f",
                "lavfi",
                "-i",
                "sine=duration=1",
                "-c:v",
                "mpeg4",
                "-shortest",
            ],
        ) else {
            return;
        };
        for hw in [HwEncoder::VideoToolbox, HwEncoder::Nvenc] {
            for (codec, name) in [(VideoCodec::H264, "h264"), (VideoCodec::Hevc, "hevc")] {
                let options = Options {
                    video_codec: Some(codec),
                    ..Options::default()
                };
                let (result, out) = convert_file(&engine, &clip, "mp4", &options, Some(hw));
                let outputs = match result {
                    Ok(outputs) => outputs,
                    Err(e) if e.to_string().contains("Unknown encoder") => {
                        eprintln!("SKIP {name}: this FFmpeg lacks its software encoder");
                        continue;
                    }
                    Err(e) => panic!("{hw:?} {name}: {e}"),
                };
                let left: Vec<_> = std::fs::read_dir(&out)
                    .unwrap()
                    .map(|e| e.unwrap().path())
                    .collect();
                assert_eq!(left, outputs, "{hw:?} {name}");
                let Some(streams) = streams(&outputs[0]) else {
                    return;
                };
                assert!(
                    streams.contains(&format!("{name},video")),
                    "{hw:?}: {streams:?}"
                );
                assert!(streams.contains(&"aac,audio".to_string()), "{streams:?}");
                std::fs::remove_dir_all(&out).unwrap();
            }
        }
        drop(dir);
    }

    /// A hardware encoder that fails during a target-size encode falls back
    /// to software for that attempt and every later one, and progress only
    /// steps back once, to the start of the attempt that fell back.
    #[test]
    fn real_ffmpeg_falls_back_from_hardware_while_meeting_a_target_size() {
        let Some(engine) = engine_with_probe() else {
            return;
        };
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("noisy.mp4");
        noisy_clip(&engine, &input, 4);
        let options = Options {
            target_size: Some(100_000),
            ..Options::default()
        };
        for hw in [HwEncoder::VideoToolbox, HwEncoder::Nvenc] {
            let step = Step {
                from: convt_core::format_by_id("mp4").unwrap(),
                to: convt_core::format_by_id("mp4").unwrap(),
            };
            let seen = Mutex::new(Vec::new());
            let progress = |p: Option<f32>| {
                if let Some(p) = p {
                    seen.lock().unwrap().push(p);
                }
            };
            let cancel = convt_core::Cancel::new();
            let ctx = Ctx::new(step, &options, &progress, &cancel);
            let out = dir.path().join(format!("out-{hw:?}"));
            std::fs::create_dir(&out).unwrap();
            let outputs = engine
                .convert_with(&ctx, &input, &out, Some(hw))
                .unwrap_or_else(|e| panic!("{hw:?}: {e}"));
            let left: Vec<_> = std::fs::read_dir(&out)
                .unwrap()
                .map(|e| e.unwrap().path())
                .collect();
            assert_eq!(left, outputs, "{hw:?}");
            let size = std::fs::metadata(&outputs[0]).unwrap().len();
            assert!(size <= 100_000, "{hw:?}: {size}");
            let seen = seen.into_inner().unwrap();
            let backs = seen.windows(2).filter(|w| w[1] < w[0]).count();
            assert!(backs <= 1, "{hw:?}: {seen:?}");
            assert!(seen.last().is_some_and(|p| *p > 0.85), "{hw:?}: {seen:?}");
        }
    }

    /// The probe only accepts an encoder that really encodes, and the full
    /// conversion works whichever path it picks.
    #[test]
    fn real_ffmpeg_probe_and_convert() {
        let Some((engine, _dir, clip)) = fixture(
            "clip.mov",
            &[
                "-f",
                "lavfi",
                "-i",
                "testsrc=size=128x96:rate=10:duration=1",
                "-c:v",
                "mpeg4",
            ],
        ) else {
            return;
        };
        let options = Options::default();
        let cancel = convt_core::Cancel::new();
        let step = Step {
            from: convt_core::format_by_id("mov").unwrap(),
            to: convt_core::format_by_id("mp4").unwrap(),
        };
        let ctx = Ctx::new(step, &options, &|_| {}, &cancel);
        let ffmpeg = engine.ffmpeg.as_ref().unwrap();
        for codec in VideoCodec::ALL {
            let found = probe_hardware(ffmpeg, codec, &ctx).unwrap();
            eprintln!("hardware {codec}: {found:?}");
            if let Some(hw) = found {
                assert!(HwEncoder::candidates().contains(&hw));
            }
        }
        let out = clip.with_file_name("out");
        std::fs::create_dir(&out).unwrap();
        let outputs = engine.convert(&ctx, &clip, &out).unwrap();
        assert!(streams(&outputs[0]).is_none_or(|s| s.contains(&"h264,video".to_string())));

        // A cancelled job stops in the probe, so no answer gets cached.
        cancel.cancel();
        assert!(matches!(
            probe_hardware(ffmpeg, VideoCodec::H264, &ctx),
            Err(Error::Cancelled)
        ));
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
