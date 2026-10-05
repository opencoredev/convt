//! What a launch asks the app to do. Requests come from the command line,
//! file associations, the Linux menus and `convt://` URLs, and a second launch
//! forwards its request to the running app.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Source {
    /// `convt-app open ...` or files passed on the command line.
    Cli,
    /// A `convt://` link. Any web page can open one, so these never start a
    /// conversion without a click.
    Url,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Request {
    #[serde(with = "wire_paths")]
    pub files: Vec<PathBuf>,
    pub to: Option<String>,
    pub preset: Option<String>,
    pub source: Option<Source>,
    /// A license key from `convt://activate?key=...`, shown in Settings for
    /// the user to activate.
    #[serde(default)]
    pub license: Option<String>,
    /// The account email from `convt://signin?email=...`, which convt.app
    /// opens after the user signs in in the browser.
    #[serde(default)]
    pub account: Option<String>,
}

impl Request {
    /// Whether the conversion may start without asking: the caller named a
    /// target or a preset, and the request didn't come from a link. Such a
    /// request converts in place with no window; a window still opens to ask
    /// if the preset has no target or a file can't be converted.
    pub fn auto_start(&self) -> bool {
        self.source == Some(Source::Cli)
            && (self.to.is_some() || self.preset.is_some())
            && !self.files.is_empty()
    }
}

pub const USAGE: &str = "\
usage: convt-app [files...]
       convt-app open [--to <format>] [--preset <name>] [--] <files...>
       convt-app 'convt://convert?file=<path>&to=<format>&preset=<name>'
       convt-app 'convt://activate?key=<license key>'
       convt-app 'convt://signin?email=<address>'

Opens the convt window. With --to, or a preset that names a format, the
files convert in place right away and no window opens.";

/// What the command line asked for.
#[derive(Debug, PartialEq, Eq)]
pub enum Command {
    Run(Request),
    Help,
    Version,
}

/// Parses the arguments after the program name. Relative paths are resolved
/// against `cwd`, since the running app may have a different one.
pub fn parse_args(args: Vec<OsString>, cwd: &Path) -> Result<Command, String> {
    let mut args = args.into_iter().peekable();
    let Some(first) = args.peek().cloned() else {
        return Ok(Command::Run(Request::default()));
    };
    match first.to_str() {
        Some("-h" | "--help" | "help") => return Ok(Command::Help),
        Some("-V" | "--version") => return Ok(Command::Version),
        Some(url) if url.starts_with("convt:") => {
            if args.len() > 1 {
                return Err("a convt:// link must be the only argument".into());
            }
            return parse_url(url).map(Command::Run);
        }
        Some("open") => {
            args.next();
        }
        _ => {}
    }
    let mut req = Request {
        source: Some(Source::Cli),
        ..Request::default()
    };
    let mut only_files = false;
    while let Some(arg) = args.next() {
        let flag = if only_files { None } else { arg.to_str() };
        let mut value = |name: &str| {
            args.next()
                .and_then(|v| v.into_string().ok())
                .ok_or_else(|| format!("{name} needs a value"))
        };
        match flag {
            Some("--") => only_files = true,
            Some("--to") => req.to = Some(value("--to")?),
            Some("--preset") => req.preset = Some(value("--preset")?),
            Some(f) if f.starts_with("--to=") => req.to = Some(f[5..].into()),
            Some(f) if f.starts_with("--preset=") => req.preset = Some(f[9..].into()),
            Some(f) if f.starts_with('-') && f.len() > 1 => {
                return Err(format!("unknown option {f}"));
            }
            // Desktop entries pass `file://` URIs (`%U`) so one entry can
            // also take `convt://` links.
            Some(f) if f.starts_with("file://") => req.files.push(file_uri(f)?),
            _ => req.files.push(cwd.join(arg)),
        }
    }
    if let Some(to) = &req.to {
        check_format(to)?;
    }
    Ok(Command::Run(req))
}

/// The local path in a `file://` URI.
fn file_uri(uri: &str) -> Result<PathBuf, String> {
    let rest = &uri["file://".len()..];
    let (host, path) = rest.split_at(rest.find('/').unwrap_or(rest.len()));
    if !matches!(host, "" | "localhost") || path.is_empty() {
        return Err(format!("{uri} is not a local file"));
    }
    Ok(PathBuf::from(bytes_to_os(percent_decode(path, false)?)))
}

fn check_format(id: &str) -> Result<(), String> {
    convt_core::format_by_id(id)
        .map(|_| ())
        .ok_or_else(|| format!("unknown format {id:?} (see `convt formats`)"))
}

/// Parses `convt://convert?file=/a&file=/b&to=png&preset=web`,
/// `convt://activate?key=...` and `convt://signin?email=...`. File paths
/// must be absolute: a link has no working directory.
pub fn parse_url(url: &str) -> Result<Request, String> {
    let rest = url
        .strip_prefix("convt://")
        .or_else(|| url.strip_prefix("convt:"))
        .ok_or("not a convt:// link")?;
    let (action, query) = rest.split_once('?').unwrap_or((rest, ""));
    let action = match action.trim_end_matches('/') {
        "convert" => Action::Convert,
        "activate" => Action::Activate,
        "signin" => Action::SignIn,
        _ => return Err(format!("unknown convt:// action {action:?}")),
    };
    let activate = action == Action::Activate;
    let convert = action == Action::Convert;
    let mut req = Request {
        source: Some(Source::Url),
        ..Request::default()
    };
    for pair in query.split('&').filter(|p| !p.is_empty()) {
        let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
        let value = percent_decode(value, true)?;
        match key {
            "key" if activate => {
                let text = String::from_utf8(value).map_err(|_| "key is not UTF-8".to_string())?;
                req.license = Some(text.trim().to_string());
            }
            "email" if action == Action::SignIn => {
                let text =
                    String::from_utf8(value).map_err(|_| "email is not UTF-8".to_string())?;
                req.account = Some(text.trim().to_string());
            }
            "file" if convert => {
                let path = PathBuf::from(bytes_to_os(value));
                if !path.is_absolute() {
                    return Err(format!("{} is not an absolute path", path.display()));
                }
                req.files.push(path);
            }
            "to" | "preset" if convert => {
                let text = String::from_utf8(value).map_err(|_| format!("{key} is not UTF-8"))?;
                if key == "to" {
                    check_format(&text)?;
                    req.to = Some(text);
                } else {
                    req.preset = Some(text);
                }
            }
            // Unknown keys are ignored so newer links still open older apps.
            _ => {}
        }
    }
    if activate && req.license.as_deref().is_none_or(str::is_empty) {
        return Err("the activate link has no key".into());
    }
    if action == Action::SignIn && req.account.as_deref().is_none_or(|e| !e.contains('@')) {
        return Err("the sign-in link has no email".into());
    }
    Ok(req)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Action {
    Convert,
    Activate,
    SignIn,
}

/// Decodes `%XX` escapes. Query strings also encode spaces as `+`; paths in
/// URIs don't.
fn percent_decode(s: &str, plus_is_space: bool) -> Result<Vec<u8>, String> {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'%' => {
                let hex = bytes
                    .get(i + 1..i + 3)
                    .and_then(|h| std::str::from_utf8(h).ok())
                    .and_then(|h| u8::from_str_radix(h, 16).ok())
                    .ok_or_else(|| format!("bad escape in {s:?}"))?;
                out.push(hex);
                i += 3;
            }
            b'+' if plus_is_space => {
                out.push(b' ');
                i += 1;
            }
            b => {
                out.push(b);
                i += 1;
            }
        }
    }
    Ok(out)
}

#[cfg(unix)]
fn bytes_to_os(bytes: Vec<u8>) -> OsString {
    use std::os::unix::ffi::OsStringExt;
    OsString::from_vec(bytes)
}

#[cfg(not(unix))]
fn bytes_to_os(bytes: Vec<u8>) -> OsString {
    String::from_utf8_lossy(&bytes).into_owned().into()
}

/// Paths travel as raw bytes on Unix, where file names need not be UTF-8.
mod wire_paths {
    use std::path::PathBuf;

    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    #[cfg(unix)]
    pub fn serialize<S: Serializer>(paths: &[PathBuf], s: S) -> Result<S::Ok, S::Error> {
        use std::os::unix::ffi::OsStrExt;
        let raw: Vec<&[u8]> = paths.iter().map(|p| p.as_os_str().as_bytes()).collect();
        raw.serialize(s)
    }

    #[cfg(unix)]
    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<PathBuf>, D::Error> {
        Ok(Vec::<Vec<u8>>::deserialize(d)?
            .into_iter()
            .map(|b| PathBuf::from(super::bytes_to_os(b)))
            .collect())
    }

    #[cfg(not(unix))]
    pub fn serialize<S: Serializer>(paths: &[PathBuf], s: S) -> Result<S::Ok, S::Error> {
        paths.serialize(s)
    }

    #[cfg(not(unix))]
    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<PathBuf>, D::Error> {
        Vec::<PathBuf>::deserialize(d)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(args: &[&str]) -> Result<Request, String> {
        let args = args.iter().map(OsString::from).collect();
        match parse_args(args, Path::new("/home/me"))? {
            Command::Run(r) => Ok(r),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn command_lines() {
        assert_eq!(run(&[]).unwrap(), Request::default());
        let r = run(&["open", "--to", "png", "a b.jpg", "/tmp/it's \"q\".heic"]).unwrap();
        assert_eq!(
            r.files,
            [
                PathBuf::from("/home/me/a b.jpg"),
                PathBuf::from("/tmp/it's \"q\".heic")
            ]
        );
        assert_eq!(r.to.as_deref(), Some("png"));
        assert!(r.auto_start());
        let r = run(&["x.png", "--preset=web"]).unwrap();
        assert_eq!(r.preset.as_deref(), Some("web"));
        assert!(r.auto_start());
        assert!(!run(&["x.png"]).unwrap().auto_start());
        let r = run(&["open", "--", "--to"]).unwrap();
        assert_eq!(r.files, [PathBuf::from("/home/me/--to")]);
        assert!(run(&["open", "--to", "nope", "a"]).is_err());
        assert!(run(&["open", "--to"]).is_err());
        assert!(run(&["--bogus"]).is_err());
        let r = run(&["file:///tmp/a%20b+%22c%22.png", "file://localhost/x.jpg"]).unwrap();
        assert_eq!(
            r.files,
            [PathBuf::from("/tmp/a b+\"c\".png"), PathBuf::from("/x.jpg")]
        );
        assert!(run(&["file://server/share/x.png"]).is_err());
        let help = parse_args(vec!["--help".into()], Path::new("/"));
        assert_eq!(help, Ok(Command::Help));
    }

    #[test]
    fn links() {
        let r =
            parse_url("convt://convert?file=%2Ftmp%2Fa%20b.png&file=/c+d.jpg&to=webp&x=1").unwrap();
        assert_eq!(
            r.files,
            [PathBuf::from("/tmp/a b.png"), PathBuf::from("/c d.jpg")]
        );
        assert_eq!(r.to.as_deref(), Some("webp"));
        // A link never starts a conversion by itself.
        assert!(!r.auto_start());
        assert!(parse_url("convt://convert?file=rel.png").is_err());
        assert!(parse_url("convt://delete?file=/a").is_err());
        assert!(parse_url("convt://convert?file=%zz").is_err());
        assert!(parse_url("convt://convert?to=nope").is_err());
        let via_args = parse_args(vec!["convt://convert?file=/a.png".into()], Path::new("/"));
        assert!(matches!(via_args, Ok(Command::Run(r)) if r.source == Some(Source::Url)));

        let r = parse_url("convt://activate?key=abc.def-_&file=/a.png").unwrap();
        assert_eq!(r.license.as_deref(), Some("abc.def-_"));
        // An activate link carries nothing else.
        assert!(r.files.is_empty());
        assert!(parse_url("convt://activate").is_err());
        assert!(parse_url("convt://activate?key=").is_err());
        assert!(
            parse_url("convt://convert?key=abc")
                .unwrap()
                .license
                .is_none()
        );

        let r =
            parse_url("convt://signin?email=a%40example.com&token=x&file=/a.png&to=png").unwrap();
        assert_eq!(r.account.as_deref(), Some("a@example.com"));
        // A sign-in link never carries files or a target.
        assert!(r.files.is_empty() && r.to.is_none() && !r.auto_start());
        assert!(parse_url("convt://signin").is_err());
        assert!(parse_url("convt://signin?email=nobody").is_err());
        assert!(
            parse_url("convt://convert?email=a@b.c")
                .unwrap()
                .account
                .is_none()
        );
    }

    #[cfg(unix)]
    #[test]
    fn non_utf8_paths_survive_the_wire() {
        use std::os::unix::ffi::OsStringExt;
        let req = Request {
            files: vec![PathBuf::from(OsString::from_vec(vec![
                b'/', 0xff, b'.', b'p',
            ]))],
            ..Request::default()
        };
        let json = serde_json::to_string(&req).unwrap();
        assert_eq!(serde_json::from_str::<Request>(&json).unwrap(), req);
    }
}
