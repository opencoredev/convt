use std::fmt::Write as _;

use clap::{ArgAction, Command};
use convt_core::{Category, FORMATS};

const TEMPLATE: &str = include_str!("../SKILL.md");

fn template() -> String {
    TEMPLATE.replace("\r\n", "\n")
}

pub(crate) fn print(cmd: &Command) {
    print!("{}", render(cmd));
}

pub(crate) fn render(cmd: &Command) -> String {
    template()
        .replace("{{FLAGS}}", &flags_markdown(cmd))
        .replace("{{COMMANDS}}", &commands_markdown(cmd))
        .replace("{{FORMATS}}", &formats_markdown())
}

fn flags_markdown(cmd: &Command) -> String {
    let mut out = String::new();
    for arg in cmd.get_arguments() {
        if let Some(line) = flag_line(arg) {
            let _ = writeln!(out, "{line}");
        }
    }
    let _ = writeln!(out, "- `--help`, `-h`: Print help");
    let _ = writeln!(out, "- `--version`, `-V`: Print version");
    out
}

fn commands_markdown(cmd: &Command) -> String {
    let mut out = String::new();
    write_commands(cmd, "convt", &mut out);
    out
}

fn write_commands(cmd: &Command, prefix: &str, out: &mut String) {
    for sub in visible_subcommands(cmd) {
        let usage = command_usage(prefix, sub);
        let _ = writeln!(out, "### `{usage}`\n");
        if let Some(about) = sub.get_about() {
            let _ = writeln!(out, "{about}\n");
        }
        for arg in sub.get_arguments() {
            if let Some(line) = flag_line(arg) {
                let _ = writeln!(out, "{line}");
            }
        }
        if sub.get_arguments().any(|arg| flag_line(arg).is_some()) {
            out.push('\n');
        }
        write_commands(sub, &format!("{prefix} {}", sub.get_name()), out);
    }
}

fn command_usage(prefix: &str, cmd: &Command) -> String {
    let mut usage = format!("{prefix} {}", cmd.get_name());
    for arg in cmd.get_arguments() {
        if arg.is_hide_set() || !arg.is_positional() {
            continue;
        }
        let name = arg
            .get_value_names()
            .and_then(|names| names.first())
            .map_or_else(
                || arg.get_id().as_str().to_ascii_uppercase(),
                |n| n.to_string(),
            );
        if arg.is_required_set() {
            usage.push_str(&format!(" <{name}>"));
        } else {
            usage.push_str(&format!(" [{name}]"));
        }
    }
    usage
}

fn flag_line(arg: &clap::Arg) -> Option<String> {
    if arg.is_hide_set() || arg.is_positional() {
        return None;
    }
    let mut names = Vec::new();
    if let Some(long) = arg.get_long() {
        names.push(format!("`--{long}`"));
    }
    if let Some(short) = arg.get_short() {
        names.push(format!("`-{short}`"));
    }
    if names.is_empty() {
        return None;
    }
    let mut line = format!("- {}", names.join(", "));
    if takes_value(arg)
        && let Some(name) = arg.get_value_names().and_then(|names| names.first())
    {
        line.push_str(&format!(" `<{name}>`"));
    }
    if let Some(help) = arg.get_help() {
        line.push_str(&format!(": {help}"));
    }
    Some(line)
}

fn takes_value(arg: &clap::Arg) -> bool {
    !matches!(
        arg.get_action(),
        ArgAction::SetTrue
            | ArgAction::SetFalse
            | ArgAction::Count
            | ArgAction::Help
            | ArgAction::HelpShort
            | ArgAction::Version
    )
}

fn formats_markdown() -> String {
    let mut out = String::new();
    let mut last = None;
    for format in FORMATS {
        if last != Some(format.category) {
            if last.is_some() {
                out.push('\n');
            }
            let _ = writeln!(out, "**{}**\n", category_label(format.category));
            last = Some(format.category);
        }
        let extensions = format
            .extensions
            .iter()
            .map(|ext| format!(".{ext}"))
            .collect::<Vec<_>>()
            .join(", ");
        let _ = writeln!(out, "- `{}` ({}): {extensions}", format.id, format.name);
    }
    out
}

fn category_label(category: Category) -> &'static str {
    match category {
        Category::Image => "Image",
        Category::Vector => "Vector",
        Category::Video => "Video",
        Category::Audio => "Audio",
        Category::Pdf => "PDF",
        Category::Document => "Document",
        Category::Presentation => "Presentation",
        Category::Spreadsheet => "Spreadsheet",
    }
}

fn visible_subcommands(cmd: &Command) -> impl Iterator<Item = &Command> {
    cmd.get_subcommands()
        .filter(|sub| !sub.is_hide_set() && sub.get_name() != "help")
}

#[cfg(test)]
use std::collections::BTreeSet;

#[cfg(test)]
pub(crate) fn clap_long_flags(cmd: &Command) -> BTreeSet<String> {
    let mut flags = BTreeSet::new();
    collect_long_flags(cmd, &mut flags);
    flags.insert("help".into());
    flags.insert("version".into());
    flags
}

#[cfg(test)]
fn collect_long_flags(cmd: &Command, flags: &mut BTreeSet<String>) {
    for arg in cmd.get_arguments() {
        if arg.is_hide_set() {
            continue;
        }
        if let Some(long) = arg.get_long() {
            flags.insert(long.to_string());
        }
    }
    for sub in visible_subcommands(cmd) {
        collect_long_flags(sub, flags);
    }
}

#[cfg(test)]
pub(crate) fn clap_subcommand_names(cmd: &Command) -> BTreeSet<String> {
    let mut names = BTreeSet::new();
    collect_subcommand_names(cmd, &mut names);
    names
}

#[cfg(test)]
fn collect_subcommand_names(cmd: &Command, names: &mut BTreeSet<String>) {
    for sub in visible_subcommands(cmd) {
        names.insert(sub.get_name().to_string());
        collect_subcommand_names(sub, names);
    }
}

#[cfg(test)]
pub(crate) fn documented_long_flags(text: &str) -> BTreeSet<String> {
    let mut flags = BTreeSet::new();
    let mut rest = text;
    while let Some(idx) = rest.find("--") {
        let after = &rest[idx + 2..];
        let end = after
            .find(|c: char| !c.is_ascii_lowercase() && !c.is_ascii_digit() && c != '-')
            .unwrap_or(after.len());
        let name = &after[..end];
        if !name.is_empty() && !name.starts_with('-') {
            flags.insert(name.to_string());
        }
        rest = if end == 0 { &after[1..] } else { &after[end..] };
    }
    flags
}

#[cfg(test)]
fn looks_like_file(token: &str) -> bool {
    token.contains('.')
        || token.contains('/')
        || token.contains('\\')
        || token.starts_with('<')
        || token.starts_with('[')
}

#[cfg(test)]
fn tokenize(invocation: &str) -> Vec<&str> {
    invocation.split_whitespace().collect()
}

#[cfg(test)]
fn each_convt_invocation(text: &str) -> Vec<Vec<String>> {
    let mut invocations = Vec::new();
    let mut rest = text;
    while let Some(idx) = rest.find("convt") {
        let before = idx
            .checked_sub(1)
            .and_then(|i| rest.get(i..idx))
            .and_then(|s| s.chars().next());
        let after = rest.get(idx + 5..).and_then(|s| s.chars().next());
        let boundary = |c: Option<char>| {
            c.is_none_or(|ch| !ch.is_ascii_alphanumeric() && ch != '-' && ch != '_')
        };
        if boundary(before) && boundary(after) {
            let tail = &rest[idx + 5..];
            let end = tail.find(['`', '\n']).unwrap_or(tail.len());
            let tokens = tokenize(tail[..end].trim());
            if !tokens.is_empty() {
                invocations.push(tokens.into_iter().map(str::to_string).collect());
            }
        }
        rest = &rest[idx + 5..];
    }
    invocations
}

/// Every `--flag` in `text` must be a real clap long option, and every bare
/// first operand after `convt` must be a real top-level subcommand.
#[cfg(test)]
pub(crate) fn assert_documented_cli_exists(cmd: &Command, text: &str) {
    let flags = clap_long_flags(cmd);
    for flag in documented_long_flags(text) {
        assert!(
            flags.contains(&flag),
            "SKILL.md documents --{flag}, which is not a CLI flag"
        );
    }

    let top_level: BTreeSet<&str> = visible_subcommands(cmd).map(Command::get_name).collect();
    for tokens in each_convt_invocation(text) {
        let Some(first) = tokens.first() else {
            continue;
        };
        if first.starts_with('-') || looks_like_file(first) {
            continue;
        }
        assert!(
            top_level.contains(first.as_str()),
            "SKILL.md documents `convt {first}`, which is not a subcommand"
        );
        let mut current = cmd
            .find_subcommand(first)
            .expect("checked against top_level");
        for token in tokens.iter().skip(1) {
            if token.starts_with('-') || looks_like_file(token) {
                break;
            }
            let has_subcommands = visible_subcommands(current).next().is_some();
            match current.find_subcommand(token) {
                Some(sub) if !sub.is_hide_set() && sub.get_name() != "help" => {
                    current = sub;
                }
                _ if has_subcommands => {
                    panic!(
                        "SKILL.md documents `convt {first} ... {token}`, which is not a subcommand"
                    );
                }
                _ => break,
            }
        }
    }
}

/// Every visible clap flag and subcommand (except clap's `help`) must appear
/// in the rendered skill so the file cannot silently drop a real option.
#[cfg(test)]
pub(crate) fn assert_cli_is_documented(cmd: &Command, text: &str) {
    let documented = documented_long_flags(text);
    for flag in clap_long_flags(cmd) {
        assert!(
            documented.contains(&flag),
            "CLI flag --{flag} is missing from SKILL.md"
        );
    }
    for name in clap_subcommand_names(cmd) {
        let listed = text.lines().any(|line| {
            line.contains("convt ")
                && line
                    .split(|c: char| c.is_whitespace() || c == '`')
                    .any(|word| word == name)
        });
        assert!(listed, "CLI subcommand {name} is missing from SKILL.md");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn template_has_frontmatter_and_placeholders() {
        let text = template();
        assert!(text.starts_with("---\nname: convt\n"));
        assert!(text.contains("description:"));
        assert!(text.contains("{{FLAGS}}"));
        assert!(text.contains("{{COMMANDS}}"));
        assert!(text.contains("{{FORMATS}}"));
        assert!(!text.contains('@'), "SKILL.md must not contain @");
        assert!(!text.contains('\r'), "skill newlines are LF");
    }

    #[test]
    fn long_flag_scan_skips_yaml_frontmatter() {
        let flags =
            documented_long_flags("---\nname: convt\n---\n\n`--to`, `--out-dir` and `--sha256`\n");
        assert_eq!(
            flags,
            BTreeSet::from(["to".into(), "out-dir".into(), "sha256".into()])
        );
    }

    #[test]
    #[should_panic(expected = "which is not a subcommand")]
    fn nested_subcommand_scan_rejects_bogus_names() {
        let cmd = Command::new("convt")
            .subcommand(Command::new("pack").subcommand(Command::new("status")));
        assert_documented_cli_exists(&cmd, "`convt pack bogus`");
    }
}
