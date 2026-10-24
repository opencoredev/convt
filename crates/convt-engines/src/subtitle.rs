use std::path::{Path, PathBuf};

use convt_core::{Ctx, Engine, Error, Result, Step};

use crate::text;

/// Converts SRT and WebVTT subtitles into each other, and either into a
/// plain-text transcript.
pub struct SubtitleEngine;

fn failed(message: impl std::fmt::Display) -> Error {
    Error::EngineFailed {
        engine: "subtitle",
        message: message.to_string(),
    }
}

impl Engine for SubtitleEngine {
    fn id(&self) -> &'static str {
        "subtitle"
    }

    // Ahead of FFmpeg, which can also rewrite subtitle files.
    fn priority(&self) -> i32 {
        10
    }

    fn steps(&self) -> Vec<Step> {
        crate::steps(&["srt", "vtt"], &["srt", "vtt", "txt"]).collect()
    }

    fn convert(&self, ctx: &Ctx, input: &Path, out_dir: &Path) -> Result<Vec<PathBuf>> {
        let source = text::read("subtitle", input)?;
        let cues = match ctx.step.from.id {
            "srt" => parse_srt(&source)?,
            _ => parse_vtt(&source)?,
        };
        let output = match ctx.step.to.id {
            "srt" => write_srt(&cues),
            "vtt" => write_vtt(&cues),
            _ => transcript(&cues),
        };
        let path = ctx.artifact(out_dir, 0);
        std::fs::write(&path, output)?;
        Ok(vec![path])
    }
}

/// One subtitle: when it shows, in milliseconds, and its text lines with
/// their markup as the source wrote it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cue {
    pub start: u64,
    pub end: u64,
    pub lines: Vec<String>,
    /// Whether the text is WebVTT, with its entities escaped.
    vtt: bool,
}

/// Blocks of lines separated by blank lines, with CRLF and CR line ends.
fn blocks(source: &str) -> Vec<Vec<(usize, &str)>> {
    let mut out = Vec::new();
    let mut block = Vec::new();
    let normalized: Vec<&str> = source
        .split("\r\n")
        .flat_map(|l| l.split(['\r', '\n']))
        .collect();
    for (i, line) in normalized.into_iter().enumerate() {
        if line.trim().is_empty() {
            if !block.is_empty() {
                out.push(std::mem::take(&mut block));
            }
        } else {
            block.push((i + 1, line));
        }
    }
    if !block.is_empty() {
        out.push(block);
    }
    out
}

/// `hh:mm:ss,mmm`, `hh:mm:ss.mmm` or `mm:ss.mmm`, in milliseconds.
fn timestamp(s: &str) -> Option<u64> {
    let (clock, fraction) = match s.find([',', '.']) {
        Some(i) => (&s[..i], &s[i + 1..]),
        None => (s, ""),
    };
    let parts: Vec<&str> = clock.split(':').collect();
    let digits = |p: &str| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit());
    if !(2..=3).contains(&parts.len()) || !parts.iter().all(|p| digits(p)) {
        return None;
    }
    if !fraction.is_empty() && !digits(fraction) {
        return None;
    }
    let n: Vec<u64> = parts
        .iter()
        .map(|p| p.parse().ok())
        .collect::<Option<_>>()?;
    let (h, m, sec) = match n[..] {
        [m, sec] => (0, m, sec),
        [h, m, sec] => (h, m, sec),
        _ => return None,
    };
    if m >= 60 || sec >= 60 {
        return None;
    }
    let ms: u64 = format!("{fraction:0<3}")[..3].parse().ok()?;
    // An absurd hour count would overflow; it is just unreadable timing.
    h.checked_mul(60)?
        .checked_add(m)?
        .checked_mul(60)?
        .checked_add(sec)?
        .checked_mul(1000)?
        .checked_add(ms)
}

/// The start and end of a timing line. Anything after the end (WebVTT cue
/// settings, SRT coordinates) is dropped.
fn timing(line: usize, s: &str) -> Result<(u64, u64)> {
    let bad = || failed(format!("line {line}: can't read the timing `{}`", s.trim()));
    let (start, rest) = s.split_once("-->").ok_or_else(bad)?;
    let end = rest.split_whitespace().next().ok_or_else(bad)?;
    let start = timestamp(start.trim()).ok_or_else(bad)?;
    let end = timestamp(end).ok_or_else(bad)?;
    Ok((start, end))
}

fn cue(block: &[(usize, &str)], vtt: bool) -> Result<Option<Cue>> {
    // An identifier or SRT number may come before the timing line.
    let Some(at) = block.iter().take(2).position(|(_, l)| l.contains("-->")) else {
        return Ok(None);
    };
    let (line, text) = block[at];
    let (start, end) = timing(line, text)?;
    Ok(Some(Cue {
        start,
        end,
        lines: block[at + 1..].iter().map(|(_, l)| l.to_string()).collect(),
        vtt,
    }))
}

fn no_cues(kind: &str) -> Error {
    failed(format!("no subtitles found in this {kind} file"))
}

pub fn parse_srt(source: &str) -> Result<Vec<Cue>> {
    let mut cues = Vec::new();
    for block in blocks(source) {
        if let Some(c) = cue(&block, false)? {
            cues.push(c);
        }
    }
    if cues.is_empty() {
        return Err(no_cues("SRT"));
    }
    Ok(cues)
}

pub fn parse_vtt(source: &str) -> Result<Vec<Cue>> {
    let mut cues = Vec::new();
    for (i, block) in blocks(source).into_iter().enumerate() {
        let first = block[0].1;
        let keyword = |k: &str| {
            first
                .strip_prefix(k)
                .is_some_and(|rest| rest.is_empty() || rest.starts_with([' ', '\t']))
        };
        // The header, comments, style sheets and region definitions are not cues.
        if (i == 0 && keyword("WEBVTT")) || keyword("NOTE") || keyword("STYLE") || keyword("REGION")
        {
            continue;
        }
        if let Some(c) = cue(&block, true)? {
            cues.push(c);
        }
    }
    if cues.is_empty() {
        return Err(no_cues("WebVTT"));
    }
    Ok(cues)
}

/// Rewrites `s`, passing each `<tag>` (without its brackets) to `tag` and
/// the text between tags to `text`. A `<` that doesn't open a tag is text.
fn rewrite(s: &str, tag: &dyn Fn(&str) -> String, text: &dyn Fn(&str) -> String) -> String {
    let mut out = String::new();
    let mut rest = s;
    let mut literal = String::new();
    while let Some(lt) = rest.find('<') {
        let after = &rest[lt + 1..];
        let opens_tag = after
            .trim_start_matches('/')
            .starts_with(|c: char| c.is_ascii_alphanumeric());
        if !opens_tag {
            literal.push_str(&rest[..=lt]);
            rest = after;
            continue;
        }
        // With no `>` left, no later `<` closes a tag either.
        let Some(gt) = after.find('>') else {
            break;
        };
        literal.push_str(&rest[..lt]);
        out.push_str(&text(&std::mem::take(&mut literal)));
        out.push_str(&tag(&after[..gt]));
        rest = &after[gt + 1..];
    }
    literal.push_str(rest);
    out.push_str(&text(&literal));
    out
}

/// `<i>`, `</b>` and so on for the styles both formats share, else nothing.
fn shared_style(tag: &str) -> String {
    let closing = tag.starts_with('/');
    let name = tag
        .trim_start_matches('/')
        .split(['.', ' ', '\t'])
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase();
    if matches!(name.as_str(), "i" | "b" | "u") {
        format!("<{}{name}>", if closing { "/" } else { "" })
    } else {
        String::new()
    }
}

/// Drops `{\an8}`-style positioning codes that many SRT files carry.
fn strip_overrides(s: &str) -> String {
    let mut out = String::new();
    let mut rest = s;
    while let Some(i) = rest.find("{\\") {
        let Some(end) = rest[i..].find('}') else {
            break;
        };
        out.push_str(&rest[..i]);
        rest = &rest[i + end + 1..];
    }
    out.push_str(rest);
    out
}

fn decode_entities(s: &str) -> String {
    let mut out = String::new();
    let mut rest = s;
    while let Some(amp) = rest.find('&') {
        out.push_str(&rest[..amp]);
        rest = &rest[amp..];
        // The longest entity, `&#x10FFFF;`, ends by byte 10, so the `;` is
        // looked for no further than that.
        let window = &rest.as_bytes()[..rest.len().min(11)];
        let semi = window.iter().position(|&b| b == b';');
        let decoded = semi.and_then(|semi| {
            let name = &rest[1..semi];
            let c = match name {
                "amp" => Some("&".to_owned()),
                "lt" => Some("<".to_owned()),
                "gt" => Some(">".to_owned()),
                "quot" => Some("\"".to_owned()),
                "apos" => Some("'".to_owned()),
                "nbsp" => Some("\u{a0}".to_owned()),
                "lrm" => Some("\u{200e}".to_owned()),
                "rlm" => Some("\u{200f}".to_owned()),
                _ => name
                    .strip_prefix("#x")
                    .or_else(|| name.strip_prefix("#X"))
                    .and_then(|h| u32::from_str_radix(h, 16).ok())
                    .or_else(|| name.strip_prefix('#').and_then(|d| d.parse().ok()))
                    .and_then(char::from_u32)
                    .map(String::from),
            }?;
            Some((c, semi))
        });
        match decoded {
            Some((c, semi)) => {
                out.push_str(&c);
                rest = &rest[semi + 1..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

fn escape_vtt(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace("-->", "--&gt;")
}

impl Cue {
    /// The lines as SRT text: italics, bold and underline kept, the rest of
    /// the markup dropped.
    fn srt_lines(&self) -> Vec<String> {
        self.clean(|line| {
            if self.vtt {
                rewrite(line, &shared_style, &decode_entities)
            } else {
                rewrite(line, &shared_style, &str::to_owned)
            }
        })
    }

    /// The lines as WebVTT cue text, escaped.
    fn vtt_lines(&self) -> Vec<String> {
        self.clean(|line| {
            if self.vtt {
                rewrite(line, &shared_style, &|t| escape_vtt(&decode_entities(t)))
            } else {
                rewrite(&strip_overrides(line), &shared_style, &escape_vtt)
            }
        })
    }

    /// The words alone.
    fn text_lines(&self) -> Vec<String> {
        let none = |_: &str| String::new();
        self.clean(|line| {
            let plain = if self.vtt {
                rewrite(line, &none, &decode_entities)
            } else {
                rewrite(&strip_overrides(line), &none, &str::to_owned)
            };
            plain.replace('\u{a0}', " ")
        })
    }

    fn clean(&self, f: impl Fn(&str) -> String) -> Vec<String> {
        self.lines
            .iter()
            .map(|l| f(l).trim().to_owned())
            .filter(|l| !l.is_empty())
            .collect()
    }
}

fn clock(ms: u64, separator: char) -> String {
    let (h, m, s, ms) = (ms / 3_600_000, ms / 60_000 % 60, ms / 1000 % 60, ms % 1000);
    format!("{h:02}:{m:02}:{s:02}{separator}{ms:03}")
}

pub fn write_srt(cues: &[Cue]) -> String {
    let mut out = String::new();
    let mut n = 0;
    for cue in cues {
        let lines = cue.srt_lines();
        if lines.is_empty() {
            continue;
        }
        n += 1;
        out.push_str(&format!(
            "{n}\n{} --> {}\n{}\n\n",
            clock(cue.start, ','),
            clock(cue.end, ','),
            lines.join("\n")
        ));
    }
    out
}

pub fn write_vtt(cues: &[Cue]) -> String {
    let mut out = String::from("WEBVTT\n\n");
    for cue in cues {
        let lines = cue.vtt_lines();
        if lines.is_empty() {
            continue;
        }
        out.push_str(&format!(
            "{} --> {}\n{}\n\n",
            clock(cue.start, '.'),
            clock(cue.end, '.'),
            lines.join("\n")
        ));
    }
    out
}

/// The dialogue, one line per subtitle line. Lines a subtitle carries over
/// from the one before, as rolling captions repeat their last line, are
/// written once; a line repeated within one subtitle stays.
pub fn transcript(cues: &[Cue]) -> String {
    let mut out: Vec<String> = Vec::new();
    let mut previous: Vec<String> = Vec::new();
    for lines in cues.iter().map(Cue::text_lines) {
        if lines.is_empty() {
            continue;
        }
        let carried = (1..=lines.len().min(previous.len()))
            .rev()
            .find(|&k| previous[previous.len() - k..] == lines[..k])
            .unwrap_or(0);
        out.extend_from_slice(&lines[carried..]);
        previous = lines;
    }
    let mut text = out.join("\n");
    if !text.is_empty() {
        text.push('\n');
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    const SRT: &str = "1\r\n00:00:01,000 --> 00:00:04,250\r\n<i>Hello</i> & welcome\r\n\r\n2\r\n00:00:05,000 --> 00:00:07,000 X1:100 X2:200\r\n{\\an8}<font color=\"#ff0\">Second</font> line\r\nTwo < three\r\n\r\n";

    #[test]
    fn srt_to_vtt_rewrites_timestamps_and_escapes() {
        let vtt = write_vtt(&parse_srt(SRT).unwrap());
        assert_eq!(
            vtt,
            "WEBVTT\n\n00:00:01.000 --> 00:00:04.250\n<i>Hello</i> &amp; welcome\n\n00:00:05.000 --> 00:00:07.000\nSecond line\nTwo &lt; three\n\n"
        );
    }

    #[test]
    fn vtt_to_srt_drops_settings_notes_and_styles() {
        let vtt = "\u{feff}WEBVTT - a title\nKind: captions\n\nSTYLE\n::cue { color: red }\n\nNOTE this is\na comment\n\nREGION\nid:fred\n\nintro\n01:02.5 --> 01:04.000 align:start position:10%\n<v Bob>Hi <c.yellow>there</c></v> &amp; &lt;you&gt;\n\n00:01:05.000 --> 00:01:06.000 line:0\n<i.loud>Loud</i> <00:01:05.500>later\n";
        let srt = write_srt(&parse_vtt(vtt).unwrap());
        assert_eq!(
            srt,
            "1\n00:01:02,500 --> 00:01:04,000\nHi there & <you>\n\n2\n00:01:05,000 --> 00:01:06,000\n<i>Loud</i> later\n\n"
        );
    }

    #[test]
    fn huge_hours_are_unreadable_timing_not_a_crash() {
        let srt = "1\n99999999999999999:00:00,000 --> 99999999999999999:00:01,000\nHi\n";
        let error = parse_srt(srt).unwrap_err().to_string();
        assert!(error.contains("line 2: can't read the timing"), "{error}");
        let vtt = "WEBVTT\n\n5124095576030431:00:00.000 --> 5124095576030431:00:01.000\nHi\n";
        assert!(parse_vtt(vtt).is_err());
        // The largest hour count that fits still reads and writes back.
        let max = u64::MAX / 3_600_000 - 1;
        let srt = format!("1\n{max}:00:00,000 --> {max}:00:01,000\nHi\n");
        let cues = parse_srt(&srt).unwrap();
        assert!(write_srt(&cues).contains(&format!("{max}:00:01,000")));
    }

    #[test]
    fn round_trips_keep_every_cue() {
        let cues = parse_srt(SRT).unwrap();
        let again = parse_srt(&write_srt(&parse_vtt(&write_vtt(&cues)).unwrap())).unwrap();
        assert_eq!(again.len(), 2);
        assert_eq!((again[1].start, again[1].end), (5000, 7000));
        assert_eq!(again[0].lines, ["<i>Hello</i> & welcome"]);
        assert_eq!(again[1].lines, ["Second line", "Two < three"]);
    }

    #[test]
    fn transcripts_strip_tags_and_collapse_repeats() {
        let srt = "1\n00:00:01,000 --> 00:00:02,000\n<i>Run!</i>\n\n2\n00:00:02,000 --> 00:00:03,000\nRun!\n\n3\n00:00:03,000 --> 00:00:04,000\n{\\an8}Now.\nRun!\n";
        assert_eq!(transcript(&parse_srt(srt).unwrap()), "Run!\nNow.\nRun!\n");
        let vtt = "WEBVTT\n\n00:01.000 --> 00:02.000\n<v Ann>Tom &amp; Jerry&nbsp;too</v>\n";
        assert_eq!(transcript(&parse_vtt(vtt).unwrap()), "Tom & Jerry too\n");
    }

    #[test]
    fn transcripts_keep_repeats_within_a_subtitle() {
        let cue =
            |t: &str, text: &str| format!("{t}\n00:00:0{t},000 --> 00:00:0{t},900\n{text}\n\n");
        let srt = [
            cue("1", "No!\nNo!"),
            cue("2", "Wait.\nNo!"),
            // Rolling captions: each one repeats the last line of the one before.
            cue("3", "one\ntwo"),
            cue("4", "two\nthree"),
            cue("5", "two\nthree"),
            cue("6", "three\nthree"),
        ]
        .concat();
        assert_eq!(
            transcript(&parse_srt(&srt).unwrap()),
            "No!\nNo!\nWait.\nNo!\none\ntwo\nthree\nthree\n"
        );
    }

    #[test]
    fn hours_past_99_and_missing_fractions() {
        assert_eq!(timestamp("100:00:00,001"), Some(360_000_001));
        assert_eq!(timestamp("00:00:02"), Some(2000));
        assert_eq!(timestamp("00:01.5"), Some(1500));
        assert_eq!(clock(360_000_001, ','), "100:00:00,001");
        assert_eq!(timestamp("00:61:00,000"), None);
        assert_eq!(timestamp("aa:00:00,000"), None);
    }

    #[test]
    fn bad_and_empty_files_fail_clearly() {
        let error = parse_srt("1\n00:00:01,000 --> soon\nHi\n").unwrap_err();
        assert!(error.to_string().contains("line 2"), "{error}");
        let error = parse_srt("").unwrap_err();
        assert!(error.to_string().contains("no subtitles"), "{error}");
        let error = parse_vtt("WEBVTT\n\nNOTE nothing here\n").unwrap_err();
        assert!(error.to_string().contains("no subtitles"), "{error}");
    }

    #[test]
    fn cues_full_of_ampersands_and_angle_brackets_stay_linear() {
        // Each `&` once looked for a `;` to the end of the text, and each
        // `<` for a `>`, so a long cue took time growing with its square.
        let n = 1_000_000;
        let lines = ["&".repeat(n), "<a".repeat(n), "< ".repeat(n)].join("\n");
        let srt = format!("1\n00:00:01,000 --> 00:00:02,000\n{lines}\nx <i>end</i> &amp;\n");
        let start = std::time::Instant::now();
        let cues = parse_srt(&srt).unwrap();
        let vtt = write_vtt(&cues);
        assert!(vtt.ends_with("x <i>end</i> &amp;amp;\n\n"));
        let back = parse_vtt(&vtt).unwrap();
        assert_eq!(back[0].lines[1].len(), "&lt;a".len() * n);
        assert_eq!(transcript(&back), transcript(&cues));
        assert!(transcript(&back).ends_with("\nx end &amp;\n"));
        // Generous for a debug build; the quadratic scans took minutes.
        assert!(start.elapsed() < std::time::Duration::from_secs(20));
        assert_eq!(
            decode_entities("&#x10FFFF;&#1114111;&amp"),
            "\u{10ffff}\u{10ffff}&amp"
        );
    }

    #[test]
    fn unicode_text_survives() {
        let srt = "1\n00:00:01,000 --> 00:00:02,000\nこんにちは 👋\n";
        let vtt = write_vtt(&parse_srt(srt).unwrap());
        assert!(vtt.contains("こんにちは 👋"));
        assert_eq!(transcript(&parse_vtt(&vtt).unwrap()), "こんにちは 👋\n");
    }

    #[test]
    fn arrows_in_text_cannot_end_a_vtt_cue() {
        let srt = "1\n00:00:01,000 --> 00:00:02,000\nleft --> right\n";
        let vtt = write_vtt(&parse_srt(srt).unwrap());
        assert!(vtt.contains("left --&gt; right"), "{vtt}");
    }
}
