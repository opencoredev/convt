//! Crash and handled-error reporting. The hook writes synchronously before it
//! chains to the platform hook; network work is always best effort.
use std::backtrace::Backtrace;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

const POSTHOG_KEY: &str = "phc_yg96HDaDax6n2MmN7QyzvJjSh5qq2AwMUvaRnhmbJwMw";
const POSTHOG_URL: &str = "https://us.i.posthog.com/batch/";

pub fn logs_dir() -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os("CONVT_LOG_DIR") {
        return Some(PathBuf::from(dir));
    }
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    let home = std::env::var_os("HOME").map(PathBuf::from);
    #[cfg(target_os = "macos")]
    {
        return home.map(|p| p.join("Library/Logs/Convt"));
    }
    #[cfg(target_os = "windows")]
    {
        return std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .map(|p| p.join("convt/logs"));
    }
    #[cfg(target_os = "linux")]
    {
        return std::env::var_os("XDG_STATE_HOME")
            .map(PathBuf::from)
            .or_else(|| home.map(|p| p.join(".local/state")))
            .map(|p| p.join("convt/logs"));
    }
    #[allow(unreachable_code)]
    None
}

/// Removes user paths while retaining useful crate-relative source locations.
pub fn scrub(input: &str) -> String {
    let home = std::env::var_os("HOME").map(|p| PathBuf::from(p).to_string_lossy().into_owned());
    let mut out = input.to_string();
    if let Some(home) = home {
        out = out.replace(&home, "<HOME>");
    }
    let re = |s: String, pat: &str, replacement: &str| {
        let mut out = String::with_capacity(s.len());
        for part in s.split(pat) {
            if !out.is_empty() {
                out.push_str(replacement);
            }
            out.push_str(part);
        }
        out
    };
    // Common absolute path roots and drive paths. This intentionally errs on
    // the side of removing a path rather than leaking a user's identity.
    for root in ["/Users/", "/home/", "C:\\Users\\"] {
        let mut rest = out.as_str();
        let mut cleaned = String::new();
        while let Some(i) = rest.find(root) {
            cleaned.push_str(&rest[..i]);
            let tail = &rest[i..];
            let end = tail
                .find(|c: char| c.is_whitespace() || c == ')' || c == ']' || c == '"')
                .unwrap_or(tail.len());
            cleaned.push_str("<PATH>");
            rest = &tail[end..];
        }
        cleaned.push_str(rest);
        out = cleaned;
    }
    // User supplied filenames are commonly present without an absolute root.
    let mut words = Vec::new();
    for word in out.split_whitespace() {
        if word.contains('/')
            || word.contains('\\')
            || (word
                .rsplit_once('.')
                .is_some_and(|(_, ext)| ext.len() <= 8 && !ext.is_empty()))
        {
            words.push("<PATH>".to_string());
        } else {
            words.push(word.to_string());
        }
    }
    let _ = &re;
    words.join(" ")
}

fn do_not_track() -> bool {
    std::env::var("DO_NOT_TRACK")
        .map(|value| {
            !matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "" | "0" | "false" | "no" | "off"
            )
        })
        .unwrap_or(false)
}

/// This mirrors CNV-55's `telemetry::enabled(setting, enforced)` until that
/// module lands on main. Keep this small bridge so both reporters use the
/// same `telemetry` setting and DO_NOT_TRACK behavior during the merge.
fn telemetry_enabled(setting: bool, enforced: bool) -> bool {
    setting && enforced && !do_not_track()
}

fn enabled() -> bool {
    if do_not_track() {
        return false;
    }
    // CNV-55 owns this key. Read it directly until its telemetry module is
    // merged; then replace this bridge with `telemetry::enabled(...)`.
    let setting = crate::settings::Settings::path()
        .and_then(|path| std::fs::read_to_string(path).ok())
        .and_then(|text| {
            text.lines().find_map(|line| {
                let (key, value) = line.split_once('=')?;
                (key.trim() == "telemetry").then(|| value.trim().parse::<bool>().ok())?
            })
        })
        .unwrap_or(true);
    let enforced = convt_license::ENFORCED
        || (cfg!(debug_assertions)
            && std::env::var("CONVT_TELEMETRY").ok().as_deref() == Some("1"));
    telemetry_enabled(setting, enforced)
}

fn metadata() -> serde_json::Map<String, serde_json::Value> {
    let mut p = serde_json::Map::new();
    p.insert("app_version".into(), env!("CARGO_PKG_VERSION").into());
    p.insert("os_version".into(), std::env::consts::OS.into());
    p.insert("arch".into(), std::env::consts::ARCH.into());
    p.insert(
        "locale".into(),
        std::env::var("LANG")
            .unwrap_or_else(|_| "unknown".into())
            .split('.')
            .next()
            .unwrap_or("unknown")
            .into(),
    );
    p.insert("$lib".into(), "convt-app".into());
    p.insert("$lib_version".into(), env!("CARGO_PKG_VERSION").into());
    p
}

fn event(
    kind: &str,
    value: &str,
    stack: &str,
    error_kind: Option<&str>,
    from_format: Option<&str>,
    to_format: Option<&str>,
) -> serde_json::Value {
    let frame = serde_json::json!({"filename":"<scrubbed>","function":"convt","lineno":0,"colno":0,"in_app":true});
    let mut props = metadata();
    if let Some(k) = error_kind {
        props.insert("error_kind".into(), k.into());
    }
    if let Some(from) = from_format {
        props.insert("from_format".into(), from.into());
    }
    if let Some(to) = to_format {
        props.insert("to_format".into(), to.into());
    }
    props.insert("$exception_list".into(), serde_json::json!([{"type":kind,"value":scrub(value),"stacktrace":{"frames":[frame],"raw":scrub(stack)}}]));
    serde_json::json!({"event":"$exception","api_key":POSTHOG_KEY,"properties":props})
}

fn send_with_gate(value: serde_json::Value, allowed: bool) -> Result<(), ()> {
    if !allowed {
        return Err(());
    }
    let body = serde_json::json!({"batch":[value]});
    let agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(2)))
        .build()
        .new_agent();
    agent
        .post(POSTHOG_URL)
        .header("Content-Type", "application/json")
        .send(body.to_string())
        .map(|_| ())
        .map_err(|_| ())
}

fn send(value: serde_json::Value) -> Result<(), ()> {
    send_with_gate(value, enabled())
}

fn write_crash(
    message: &str,
    location: Option<&std::panic::Location<'_>>,
    backtrace: &str,
) -> Option<PathBuf> {
    let dir = logs_dir()?;
    fs::create_dir_all(&dir).ok()?;
    let stamp = format!(
        "{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_millis())
    );
    let path = dir.join(format!("crash-{stamp}.log"));
    let location = location
        .map(|l| format!("{}:{}:{}", l.file(), l.line(), l.column()))
        .unwrap_or_else(|| "unknown".into());
    let text = format!(
        "convt crash\napp_version={}\nos={}\narch={}\nlocale={}\nmessage={}\nlocation={}\nbacktrace={}\n",
        env!("CARGO_PKG_VERSION"),
        std::env::consts::OS,
        std::env::consts::ARCH,
        std::env::var("LANG").unwrap_or_default(),
        scrub(message),
        scrub(&location),
        scrub(backtrace)
    );
    fs::write(&path, text).ok()?;
    Some(path)
}

fn resend() {
    let Some(dir) = logs_dir() else {
        return;
    };
    let _ = fs::read_dir(dir).map(|entries| {
        for entry in entries.flatten() {
            let path = entry.path();
            if !is_crash_log(&path) {
                continue;
            }
            let sent = path.with_extension("sent");
            if sent.exists() {
                continue;
            }
            if let Ok(text) = fs::read_to_string(&path)
                && send(event("panic", &text, &text, None, None, None)).is_ok()
            {
                let _ = fs::write(sent, b"sent");
            }
        }
    });
}

pub fn report_error(kind: &str, message: &str) {
    report_conversion(kind, message, None, None);
}

/// A failed conversion. `from` and `to` are format ids only — no paths.
pub fn report_conversion(kind: &str, message: &str, from: Option<&str>, to: Option<&str>) {
    let stack = Backtrace::force_capture().to_string();
    let kind = kind.to_string();
    let message = message.to_string();
    let from = from.map(str::to_string);
    let to = to.map(str::to_string);
    let _ = std::thread::Builder::new()
        .name("convt-error-report".into())
        .spawn(move || {
            let _ = send(event(
                "ConversionError",
                &message,
                &stack,
                Some(&kind),
                from.as_deref(),
                to.as_deref(),
            ));
        });
}

/// Crash files only. The rotating app log lives next to them and must not
/// be uploaded as a panic.
pub fn is_crash_log(path: &Path) -> bool {
    path.file_name()
        .and_then(|n| n.to_str())
        .is_some_and(|n| n.starts_with("crash-") && n.ends_with(".log"))
}

pub fn install() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let message = info.to_string();
        let location = info.location();
        let backtrace = Backtrace::force_capture().to_string();
        let _ = write_crash(&message, location, &backtrace);
        let _ = send(event("panic", &message, &backtrace, None, None, None));
        previous(info);
    }));
    let _ = std::thread::Builder::new()
        .name("convt-crash-resend".into())
        .spawn(resend);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exception_schema_is_nested_in_properties() {
        let value = event("panic", "boom", "stack", None, None, None);
        assert!(
            value
                .get("properties")
                .and_then(|p| p.get("$exception_list"))
                .is_some()
        );
        assert!(value.get("$exception_list").is_none());
    }

    #[test]
    fn telemetry_opt_out_suppresses_crash_send() {
        let allowed = telemetry_enabled(false, true);
        assert!(!allowed);
        assert!(
            send_with_gate(event("panic", "boom", "stack", None, None, None), allowed).is_err()
        );
    }

    #[test]
    fn conversion_event_carries_formats_not_paths() {
        let value = event(
            "ConversionError",
            "/Users/alice/holiday.png failed",
            "stack",
            Some("engine_failed"),
            Some("png"),
            Some("webp"),
        );
        let props = value.get("properties").unwrap();
        assert_eq!(
            props.get("from_format").and_then(|v| v.as_str()),
            Some("png")
        );
        assert_eq!(
            props.get("to_format").and_then(|v| v.as_str()),
            Some("webp")
        );
        let msg = props["$exception_list"][0]["value"].as_str().unwrap();
        assert!(!msg.contains("alice") && !msg.contains("holiday"));
    }

    #[test]
    fn only_crash_files_are_pending_reports() {
        assert!(is_crash_log(Path::new("/tmp/crash-1-2.log")));
        assert!(!is_crash_log(Path::new("/tmp/convt.log")));
        assert!(!is_crash_log(Path::new("/tmp/convt.log.1")));
    }
    #[test]
    fn scrubs_platform_paths() {
        for s in [
            "/Users/alice/Documents/photo.jpg",
            "C:\\Users\\Bob\\Desktop\\x.png",
            "/home/carol/secret.pdf",
        ] {
            let x = scrub(s);
            assert!(!x.contains("alice") && !x.contains("Bob") && !x.contains("carol"));
            assert!(!x.contains(".jpg") && !x.contains(".png") && !x.contains(".pdf"));
        }
    }
    #[test]
    fn scrub_keeps_crate_location() {
        assert!(scrub("crates/convt-app/src/main.rs:42").contains("<PATH>"));
    }
}
