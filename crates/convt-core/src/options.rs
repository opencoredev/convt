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
