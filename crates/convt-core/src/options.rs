use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::{Error, Result};

/// Settings for a conversion. Every field is optional; engines ignore the
/// ones that don't apply to their output.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Options {
    /// Lossy quality from 1 to 100 for JPEG, AVIF and video.
    pub quality: Option<u8>,
    /// Longest edge in pixels for image output. Larger images are scaled down.
    pub max_size: Option<u32>,
    /// Maximum output video height in pixels. Scales down only, keeps the
    /// aspect ratio, and rounds both dimensions down to even pixels.
    pub video_height: Option<u32>,
    /// Audio bitrate in kbit/s.
    pub audio_bitrate: Option<u32>,
    /// Pages to render from a PDF, e.g. `"1-3"`, `"2"` or `"4-"`.
    pub pages: Option<PageRange>,
    /// Resolution for rendering PDF and SVG pages.
    pub dpi: Option<u32>,
    /// Video encoder for MP4, MOV and MKV output. Unset means H.264.
    pub video_codec: Option<VideoCodec>,
    /// What transparent areas of an image become. Unset keeps transparency
    /// where the output format can store it and uses white where it can't.
    pub background: Option<Background>,
    /// Leave the audio out of video output.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub strip_audio: bool,
    /// Largest output file in bytes. Engines lower quality (and for images,
    /// dimensions) until the output fits. Presets may write `"10MB"`.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "size_from_toml"
    )]
    pub target_size: Option<u64>,
    /// Leave EXIF, XMP, GPS and container metadata out of the output. Color
    /// profiles stay, since dropping one changes the colors. Images are
    /// rotated to their EXIF orientation first.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub strip_metadata: bool,
    /// Where video and audio output starts in the input.
    pub trim_start: Option<Timestamp>,
    /// Where video and audio output ends in the input.
    pub trim_end: Option<Timestamp>,
    /// Frames per second for video and animated GIF output.
    pub fps: Option<u32>,
}

/// The smallest `target_size` engines accept: below this, container
/// overhead alone can exceed the budget.
pub const MIN_TARGET_SIZE: u64 = 10_000;

/// Parses a file size: `10MB`, `8M`, `500KB`, `1.5 GB`, `2MiB` or plain
/// bytes. `K`, `M` and `G` are decimal (1000), so a "10 MB" upload limit is
/// met whichever way the site counts; `KiB`, `MiB` and `GiB` are binary.
pub fn parse_size(s: &str) -> Result<u64> {
    let bad = || Error::InvalidOption(format!("size {s:?}; use bytes or e.g. 500KB, 8MB, 1.5GB"));
    let t = s.trim();
    let split = t
        .find(|c: char| !(c.is_ascii_digit() || c == '.'))
        .unwrap_or(t.len());
    let (num, unit) = t.split_at(split);
    let num: f64 = num.parse().map_err(|_| bad())?;
    let scale: f64 = match unit.trim().to_ascii_lowercase().as_str() {
        "" | "b" => 1.0,
        "k" | "kb" => 1e3,
        "m" | "mb" => 1e6,
        "g" | "gb" => 1e9,
        "kib" => 1024.0,
        "mib" => 1024.0 * 1024.0,
        "gib" => 1024.0 * 1024.0 * 1024.0,
        _ => return Err(bad()),
    };
    let bytes = (num * scale).floor();
    if !bytes.is_finite() || bytes < 1.0 || bytes > u64::MAX as f64 {
        return Err(bad());
    }
    Ok(bytes as u64)
}

/// Formats bytes the way [`parse_size`] reads them: `10MB`, `500KB`, `1234B`.
pub fn format_size(bytes: u64) -> String {
    for (unit, scale) in [("GB", 1_000_000_000), ("MB", 1_000_000), ("KB", 1000)] {
        if bytes >= scale && bytes.is_multiple_of(scale) {
            return format!("{}{unit}", bytes / scale);
        }
    }
    format!("{bytes}B")
}

fn size_from_toml<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> std::result::Result<Option<u64>, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Size {
        Bytes(u64),
        Text(String),
    }
    match Size::deserialize(d)? {
        Size::Bytes(b) => Ok(Some(b)),
        Size::Text(s) => parse_size(&s).map(Some).map_err(serde::de::Error::custom),
    }
}

/// A point in a video or audio file, to the millisecond. Written as seconds
/// (`90`, `12.5`) or a timestamp (`1:30`, `01:02:03.250`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Timestamp {
    pub millis: u64,
}

impl Timestamp {
    pub fn from_secs_f64(secs: f64) -> Option<Self> {
        (0.0..1e12).contains(&secs).then(|| Timestamp {
            millis: (secs * 1000.0).round() as u64,
        })
    }

    pub fn as_secs_f64(self) -> f64 {
        self.millis as f64 / 1000.0
    }
}

impl std::str::FromStr for Timestamp {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self> {
        let bad = || {
            Error::InvalidOption(format!(
                "time {s:?}; use seconds like 12.5 or a timestamp like 1:30 or 01:02:03.250"
            ))
        };
        let parts: Vec<&str> = s.trim().split(':').collect();
        if parts.len() > 3 || parts.iter().any(|p| p.is_empty()) {
            return Err(bad());
        }
        let (last, whole) = parts.split_last().ok_or_else(bad)?;
        let mut secs = 0.0;
        for p in whole {
            if !p.bytes().all(|b| b.is_ascii_digit()) {
                return Err(bad());
            }
            secs = secs * 60.0 + p.parse::<f64>().map_err(|_| bad())?;
        }
        if !last.bytes().all(|b| b.is_ascii_digit() || b == b'.') {
            return Err(bad());
        }
        let tail: f64 = last.parse().map_err(|_| bad())?;
        if !whole.is_empty() && tail >= 60.0 {
            return Err(bad());
        }
        Timestamp::from_secs_f64(secs * 60.0 + tail).ok_or_else(bad)
    }
}

impl std::fmt::Display for Timestamp {
    /// Seconds with up to three decimals, which FFmpeg reads as-is.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let (s, ms) = (self.millis / 1000, self.millis % 1000);
        if ms == 0 {
            write!(f, "{s}")
        } else {
            let frac = format!("{ms:03}");
            write!(f, "{s}.{}", frac.trim_end_matches('0'))
        }
    }
}

impl Serialize for Timestamp {
    fn serialize<S: serde::Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
        s.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for Timestamp {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> std::result::Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Time {
            Int(u64),
            Float(f64),
            Text(String),
        }
        let bad =
            |what: String| serde::de::Error::custom(format!("time {what}; use seconds or 1:30"));
        match Time::deserialize(d)? {
            Time::Int(n) => Timestamp::from_secs_f64(n as f64).ok_or_else(|| bad(n.to_string())),
            Time::Float(x) => Timestamp::from_secs_f64(x).ok_or_else(|| bad(x.to_string())),
            Time::Text(s) => s.parse().map_err(serde::de::Error::custom),
        }
    }
}

/// A video encoder the user can pick for MP4, MOV and MKV output.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum VideoCodec {
    /// H.264 (AVC): plays nearly everywhere.
    H264,
    /// H.265 (HEVC): smaller files, newer players.
    Hevc,
}

impl VideoCodec {
    pub const ALL: [VideoCodec; 2] = [VideoCodec::H264, VideoCodec::Hevc];

    /// The id presets and the CLI use: `h264` or `hevc`.
    pub fn id(self) -> &'static str {
        match self {
            VideoCodec::H264 => "h264",
            VideoCodec::Hevc => "hevc",
        }
    }

    /// The name people know it by.
    pub fn name(self) -> &'static str {
        match self {
            VideoCodec::H264 => "H.264",
            VideoCodec::Hevc => "HEVC",
        }
    }
}

impl std::str::FromStr for VideoCodec {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self> {
        match s.to_ascii_lowercase().as_str() {
            "h264" | "h.264" | "avc" => Ok(VideoCodec::H264),
            "hevc" | "h265" | "h.265" => Ok(VideoCodec::Hevc),
            _ => Err(Error::InvalidOption(format!(
                "video codec {s:?}; use h264 or hevc"
            ))),
        }
    }
}

impl std::fmt::Display for VideoCodec {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.id())
    }
}

impl Options {
    pub fn validate(&self) -> Result<()> {
        let bad = |m: &str| Err(Error::InvalidOption(m.into()));
        if self.quality.is_some_and(|q| !(1..=100).contains(&q)) {
            return bad("quality must be between 1 and 100");
        }
        if self.max_size == Some(0) || self.video_height == Some(0) {
            return bad("sizes must be at least 1 pixel");
        }
        if self.video_height.is_some_and(|h| h % 2 == 1) {
            return bad("video height must be even");
        }
        if self.audio_bitrate.is_some_and(|b| !(8..=640).contains(&b)) {
            return bad("audio bitrate must be between 8 and 640 kbit/s");
        }
        if self.dpi.is_some_and(|d| !(18..=1200).contains(&d)) {
            return bad("dpi must be between 18 and 1200");
        }
        if self.target_size.is_some_and(|b| b < MIN_TARGET_SIZE) {
            return bad("target size must be at least 10KB");
        }
        if self.fps.is_some_and(|f| !(1..=120).contains(&f)) {
            return bad("fps must be between 1 and 120");
        }
        if self.trim_end.is_some_and(|end| end.millis == 0) {
            return bad("trim end must be after the start of the file");
        }
        if let (Some(start), Some(end)) = (self.trim_start, self.trim_end)
            && end <= start
        {
            return bad("trim end must come after trim start");
        }
        Ok(())
    }

    /// Fills every unset field from `base`.
    pub fn or(self, base: &Options) -> Options {
        Options {
            quality: self.quality.or(base.quality),
            max_size: self.max_size.or(base.max_size),
            video_height: self.video_height.or(base.video_height),
            audio_bitrate: self.audio_bitrate.or(base.audio_bitrate),
            pages: self.pages.or(base.pages),
            dpi: self.dpi.or(base.dpi),
            video_codec: self.video_codec.or(base.video_codec),
            background: self.background.or(base.background),
            strip_audio: self.strip_audio || base.strip_audio,
            target_size: self.target_size.or(base.target_size),
            strip_metadata: self.strip_metadata || base.strip_metadata,
            trim_start: self.trim_start.or(base.trim_start),
            trim_end: self.trim_end.or(base.trim_end),
            fps: self.fps.or(base.fps),
        }
    }
}

/// What transparent areas of an image become: kept transparent, or flattened
/// onto a solid color. Written `transparent`, `white`, `black` or `#rrggbb`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Background {
    Transparent,
    Color([u8; 3]),
}

impl Background {
    pub const WHITE: Background = Background::Color([255, 255, 255]);
    pub const BLACK: Background = Background::Color([0, 0, 0]);
    /// The choices the app offers; any other color works from the CLI and presets.
    pub const CHOICES: [Background; 3] = [
        Background::Transparent,
        Background::WHITE,
        Background::BLACK,
    ];

    /// The id presets and the CLI use: `transparent`, `white`, `black` or `#rrggbb`.
    pub fn id(self) -> String {
        match self {
            Background::Transparent => "transparent".into(),
            Background::WHITE => "white".into(),
            Background::BLACK => "black".into(),
            Background::Color([r, g, b]) => format!("#{r:02x}{g:02x}{b:02x}"),
        }
    }

    /// The name people see: `Transparent`, `White`, `Black` or `#RRGGBB`.
    pub fn name(self) -> String {
        match self {
            Background::Transparent => "Transparent".into(),
            Background::WHITE => "White".into(),
            Background::BLACK => "Black".into(),
            Background::Color(_) => self.id().to_ascii_uppercase(),
        }
    }
}

impl std::str::FromStr for Background {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self> {
        let bad = || {
            Error::InvalidOption(format!(
                "background {s:?}; use transparent, white, black or a color like #ff8800"
            ))
        };
        let lower = s.trim().to_ascii_lowercase();
        match lower.as_str() {
            "transparent" => return Ok(Background::Transparent),
            "white" => return Ok(Background::WHITE),
            "black" => return Ok(Background::BLACK),
            _ => {}
        }
        let hex = lower.strip_prefix('#').unwrap_or(&lower);
        let digits: Vec<u8> = hex
            .chars()
            .map(|c| c.to_digit(16).map(|d| d as u8))
            .collect::<Option<_>>()
            .ok_or_else(bad)?;
        match digits[..] {
            // #rgb is shorthand for #rrggbb.
            [r, g, b] => Ok(Background::Color([r * 17, g * 17, b * 17])),
            [r1, r2, g1, g2, b1, b2] => Ok(Background::Color([
                r1 * 16 + r2,
                g1 * 16 + g2,
                b1 * 16 + b2,
            ])),
            _ => Err(bad()),
        }
    }
}

impl std::fmt::Display for Background {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.id())
    }
}

impl Serialize for Background {
    fn serialize<S: serde::Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
        s.serialize_str(&self.id())
    }
}

impl<'de> Deserialize<'de> for Background {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> std::result::Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        s.parse().map_err(serde::de::Error::custom)
    }
}

/// An inclusive, 1-based page range. `last: None` runs to the end.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PageRange {
    pub first: u32,
    pub last: Option<u32>,
}

impl PageRange {
    /// Whether 0-based page `index` is in the range.
    pub fn contains(&self, index: usize) -> bool {
        let page = index as u64 + 1;
        page >= self.first as u64 && self.last.is_none_or(|l| page <= l as u64)
    }
}

impl std::str::FromStr for PageRange {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self> {
        let bad = || Error::InvalidOption(format!("page range {s:?}; use 2, 1-3 or 4-"));
        let num = |t: &str| t.trim().parse::<u32>().ok().filter(|n| *n > 0);
        let range = match s.split_once('-') {
            None => {
                let n = num(s).ok_or_else(bad)?;
                PageRange {
                    first: n,
                    last: Some(n),
                }
            }
            Some((a, b)) => PageRange {
                first: num(a).ok_or_else(bad)?,
                last: if b.trim().is_empty() {
                    None
                } else {
                    Some(num(b).ok_or_else(bad)?)
                },
            },
        };
        if range.last.is_some_and(|l| l < range.first) {
            return Err(bad());
        }
        Ok(range)
    }
}

impl std::fmt::Display for PageRange {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.last {
            Some(l) if l == self.first => write!(f, "{l}"),
            Some(l) => write!(f, "{}-{l}", self.first),
            None => write!(f, "{}-", self.first),
        }
    }
}

impl Serialize for PageRange {
    fn serialize<S: serde::Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
        s.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for PageRange {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> std::result::Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        s.parse().map_err(serde::de::Error::custom)
    }
}

/// A named set of options, optionally tied to a target format. Presets are
/// TOML files:
///
/// ```toml
/// to = "jpeg"
/// quality = 80
/// max_size = 2048
/// ```
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Preset {
    /// Target format id. A preset without one works with `--to`.
    pub to: Option<String>,
    pub options: Options,
}

impl Preset {
    pub fn parse(toml_text: &str) -> Result<Self> {
        let invalid = |e: toml::de::Error| Error::InvalidOption(e.message().into());
        let mut table: toml::Table = toml::from_str(toml_text).map_err(invalid)?;
        let to = match table.remove("to") {
            None => None,
            Some(toml::Value::String(s)) => Some(s),
            Some(_) => return Err(Error::InvalidOption("`to` must be a format id".into())),
        };
        let preset = Preset {
            to,
            options: table.try_into().map_err(invalid)?,
        };
        if let Some(to) = &preset.to {
            crate::format_by_id(to).ok_or_else(|| Error::UnknownFormat(to.clone()))?;
        }
        preset.options.validate()?;
        Ok(preset)
    }

    /// Serializes the preset in the format [`Preset::parse`] reads.
    pub fn to_toml(&self) -> String {
        let mut table = toml::Table::new();
        if let Some(to) = &self.to {
            table.insert("to".into(), toml::Value::String(to.clone()));
        }
        if let Ok(toml::Value::Table(options)) = toml::Value::try_from(&self.options) {
            table.extend(options);
        }
        toml::to_string(&table).expect("a TOML table always serializes")
    }

    /// Reads and parses one preset file. Errors name the file.
    pub fn load(path: &Path) -> Result<Self> {
        std::fs::read_to_string(path)
            .map_err(Error::from)
            .and_then(|text| Preset::parse(&text))
            .map_err(|e| {
                let why = match e {
                    Error::InvalidOption(msg) => msg,
                    e => e.to_string(),
                };
                Error::InvalidOption(format!("preset {}: {why}", path.display()))
            })
    }

    /// Loads every `*.toml` file in `dir`, keyed by file stem. Each file gets
    /// its own result, so one broken preset doesn't hide the others. A missing
    /// directory is empty, not an error.
    pub fn load_dir(dir: &Path) -> Result<BTreeMap<String, Result<Preset>>> {
        let mut out = BTreeMap::new();
        let entries = match std::fs::read_dir(dir) {
            Ok(e) => e,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(out),
            Err(e) => return Err(e.into()),
        };
        for entry in entries {
            let path = entry?.path();
            if path.extension().is_some_and(|e| e == "toml")
                && let Some(name) = path.file_stem().and_then(|s| s.to_str())
            {
                out.insert(name.to_string(), Preset::load(&path));
            }
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn page_ranges() {
        let r: PageRange = "2-3".parse().unwrap();
        assert!(!r.contains(0) && r.contains(1) && r.contains(2) && !r.contains(3));
        let r: PageRange = "4-".parse().unwrap();
        assert!(!r.contains(2) && r.contains(3) && r.contains(500));
        assert_eq!("5".parse::<PageRange>().unwrap().to_string(), "5");
        for bad in ["0", "3-1", "a", "-2", ""] {
            assert!(bad.parse::<PageRange>().is_err(), "{bad}");
        }
    }

    #[test]
    fn presets() {
        let p = Preset::parse("to = \"jpeg\"\nquality = 80\npages = \"1-2\"").unwrap();
        assert_eq!(p.to.as_deref(), Some("jpeg"));
        assert_eq!(p.options.quality, Some(80));
        assert_eq!(p.options.pages, Some("1-2".parse().unwrap()));
        assert!(Preset::parse("quality = 0").is_err());
        assert!(Preset::parse("qualty = 50").is_err());
        assert!(Preset::parse("to = \"nope\"").is_err());
        assert_eq!(Preset::parse(&p.to_toml()).unwrap(), p);
        assert_eq!(Preset::default().to_toml(), "");
        let merged = Options {
            quality: Some(50),
            ..Default::default()
        }
        .or(&p.options);
        assert_eq!((merged.quality, merged.pages), (Some(50), p.options.pages));
    }

    #[test]
    fn video_codec_and_strip_audio() {
        // Old presets load with the defaults, and defaults don't serialize.
        let old = Preset::parse("to = \"mp4\"\nquality = 70").unwrap();
        assert_eq!(old.options.video_codec, None);
        assert!(!old.options.strip_audio);
        assert!(!old.to_toml().contains("codec") && !old.to_toml().contains("audio"));

        let p = Preset::parse("to = \"mkv\"\nvideo_codec = \"hevc\"\nstrip_audio = true").unwrap();
        assert_eq!(p.options.video_codec, Some(VideoCodec::Hevc));
        assert!(p.options.strip_audio);
        assert_eq!(Preset::parse(&p.to_toml()).unwrap(), p);
        assert!(Preset::parse("video_codec = \"vp8\"").is_err());
        assert!(Preset::parse("strip_audio = \"yes\"").is_err());

        assert_eq!("H.264".parse::<VideoCodec>().unwrap(), VideoCodec::H264);
        assert_eq!("hevc".parse::<VideoCodec>().unwrap(), VideoCodec::Hevc);
        assert!("av1".parse::<VideoCodec>().is_err());
        for codec in VideoCodec::ALL {
            assert_eq!(codec.id().parse::<VideoCodec>().unwrap(), codec);
        }

        let merged = Options::default().or(&p.options);
        assert_eq!(merged.video_codec, Some(VideoCodec::Hevc));
        assert!(merged.strip_audio);
        assert!(merged.validate().is_ok());
    }

    #[test]
    fn backgrounds() {
        for (text, want) in [
            ("transparent", Background::Transparent),
            ("White", Background::WHITE),
            ("BLACK", Background::BLACK),
            ("#ff8800", Background::Color([255, 136, 0])),
            ("FF8800", Background::Color([255, 136, 0])),
            ("#08f", Background::Color([0, 136, 255])),
        ] {
            assert_eq!(text.parse::<Background>().unwrap(), want, "{text}");
        }
        for bad in ["", "#12", "#1234567", "#gggggg", "grey", "#"] {
            assert!(bad.parse::<Background>().is_err(), "{bad}");
        }
        // Ids round-trip, and named colors keep their names.
        for b in [
            Background::Transparent,
            Background::WHITE,
            Background::BLACK,
            Background::Color([1, 2, 255]),
        ] {
            assert_eq!(b.id().parse::<Background>().unwrap(), b);
        }
        assert_eq!(Background::Color([255, 255, 255]).id(), "white");
        assert_eq!(Background::Color([1, 2, 255]).name(), "#0102FF");

        let p = Preset::parse("to = \"jpeg\"\nbackground = \"black\"").unwrap();
        assert_eq!(p.options.background, Some(Background::BLACK));
        assert_eq!(Preset::parse(&p.to_toml()).unwrap(), p);
        assert!(Preset::parse("background = \"grey\"").is_err());
        assert!(
            !Preset::parse("quality = 50")
                .unwrap()
                .to_toml()
                .contains("background")
        );
        let merged = Options::default().or(&p.options);
        assert_eq!(merged.background, Some(Background::BLACK));
    }

    #[test]
    fn sizes() {
        for (text, want) in [
            ("10MB", 10_000_000),
            ("8M", 8_000_000),
            ("500KB", 500_000),
            ("500 kb", 500_000),
            ("1.5GB", 1_500_000_000),
            ("2MiB", 2 * 1024 * 1024),
            ("12345", 12_345),
            ("100B", 100),
        ] {
            assert_eq!(parse_size(text).unwrap(), want, "{text}");
        }
        for bad in ["", "MB", "-1MB", "10 XB", "1.2.3MB", "0", "0.1"] {
            assert!(parse_size(bad).is_err(), "{bad}");
        }
        for bytes in [10_000_000, 500_000, 2_000_000_000, 12_345] {
            assert_eq!(parse_size(&format_size(bytes)).unwrap(), bytes);
        }
        assert_eq!(format_size(25_000_000), "25MB");

        let p = Preset::parse("to = \"mp4\"\ntarget_size = \"10MB\"").unwrap();
        assert_eq!(p.options.target_size, Some(10_000_000));
        assert_eq!(Preset::parse(&p.to_toml()).unwrap(), p);
        let p = Preset::parse("target_size = 25000000").unwrap();
        assert_eq!(p.options.target_size, Some(25_000_000));
        assert!(Preset::parse("target_size = \"big\"").is_err());
        assert!(
            Preset::parse("target_size = 100").is_err(),
            "below the minimum"
        );
        assert!(!Preset::default().to_toml().contains("target_size"));
    }

    #[test]
    fn timestamps() {
        for (text, millis) in [
            ("90", 90_000),
            ("12.5", 12_500),
            ("1:30", 90_000),
            ("01:02:03.250", 3_723_250),
            ("0:00.001", 1),
        ] {
            let t: Timestamp = text.parse().unwrap();
            assert_eq!(t.millis, millis, "{text}");
            assert_eq!(t.to_string().parse::<Timestamp>().unwrap(), t);
        }
        for bad in [
            "", "-1", "1:", ":30", "1:60", "1:2:3:4", "abc", "1e3", "1:-5",
        ] {
            assert!(bad.parse::<Timestamp>().is_err(), "{bad}");
        }
        assert_eq!("12.5".parse::<Timestamp>().unwrap().to_string(), "12.5");
        assert_eq!("1:30".parse::<Timestamp>().unwrap().to_string(), "90");

        let p = Preset::parse("trim_start = \"0:05\"\ntrim_end = 12.5\nfps = 24").unwrap();
        assert_eq!(p.options.trim_start.unwrap().millis, 5_000);
        assert_eq!(p.options.trim_end.unwrap().millis, 12_500);
        assert_eq!(p.options.fps, Some(24));
        assert_eq!(Preset::parse(&p.to_toml()).unwrap(), p);
        assert!(Preset::parse("trim_start = 10\ntrim_end = 5").is_err());
        assert!(Preset::parse("trim_end = 0").is_err());
        assert!(Preset::parse("fps = 0").is_err());
        assert!(Preset::parse("fps = 500").is_err());
    }

    #[test]
    fn strip_metadata_round_trips_and_merges() {
        assert!(
            !Preset::parse("quality = 50")
                .unwrap()
                .to_toml()
                .contains("strip_metadata")
        );
        let p = Preset::parse("strip_metadata = true").unwrap();
        assert!(p.options.strip_metadata);
        assert_eq!(Preset::parse(&p.to_toml()).unwrap(), p);
        assert!(Options::default().or(&p.options).strip_metadata);
    }

    #[test]
    fn a_broken_preset_does_not_hide_the_rest() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("good.toml"), "to = \"png\"").unwrap();
        std::fs::write(dir.path().join("bad.toml"), "to = \"zzz\"").unwrap();
        std::fs::write(dir.path().join("notes.txt"), "").unwrap();
        let all = Preset::load_dir(dir.path()).unwrap();
        assert_eq!(all.len(), 2);
        assert!(all["good"].is_ok() && all["bad"].is_err());
    }
}
