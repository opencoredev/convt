//! Settings → General → Support, and the Help-menu actions PR #68 can wire
//! into a real macOS menu bar.
//!
//! Isolated on purpose: the UI rewrite restyles Settings and adds menus.
//! Keep this file as the one place for Copy logs and Reveal log file.

use gpui_kit::*;

use super::theme::{Palette, secondary_button, text};
use crate::model::AppState;

/// Copy logs / Reveal log file, for the General tab.
pub fn settings_section(app: &Entity<AppState>, p: &Palette, cx: &App) -> Div {
    let notice = app.read(cx).logs_notice.clone();
    div()
        .flex()
        .flex_col()
        .gap(px(12.))
        .child(text(12., 16., p.secondary).child(
            "Copy a diagnostics bundle (app version, OS, arch, license state, recent logs) \
             or open the log folder. Paths are scrubbed. Crash and conversion errors use \
             the existing reporter; this does not send anything.",
        ))
        .child(
            div()
                .flex()
                .flex_wrap()
                .gap(px(10.))
                .child(secondary_button("copy-logs", "Copy logs", p).on_click({
                    let app = app.clone();
                    move |_, _, cx| copy_logs(&app, cx)
                }))
                .child(
                    secondary_button("reveal-log-file", "Reveal log file", p).on_click({
                        let app = app.clone();
                        move |_, _, cx| reveal_log_file(&app, cx)
                    }),
                ),
        )
        .children(notice.map(|n| {
            div()
                .id("logs-notice")
                .test_support()
                .aria_label(SharedString::from(n.clone()))
                .child(text(12., 16., p.secondary).child(n))
        }))
}

/// Builds the diagnostics bundle and puts it on the clipboard.
pub fn copy_logs(app: &Entity<AppState>, cx: &mut App) {
    app.update(cx, |s, cx| {
        let bundle = s.diagnostics_bundle();
        cx.write_to_clipboard(ClipboardItem::new_string(bundle));
        s.logs_notice = Some("Copied. Paths are scrubbed; paste this wherever you want.".into());
        cx.notify();
    });
}

/// Opens the folder that holds `convt.log`.
pub fn reveal_log_file(app: &Entity<AppState>, cx: &mut App) {
    app.update(cx, |s, cx| s.reveal_logs(cx));
}

/// Registers the Help-menu actions. Call from `main` (and again from PR #68
/// when it builds the macOS menu bar). Safe to call more than once.
pub fn register_actions(cx: &mut App) {
    cx.on_action(|_: &CopyLogs, cx| {
        if let Some(app) = crate::model::try_shared(cx) {
            copy_logs(&app, cx);
        }
    });
    cx.on_action(|_: &RevealLogFile, cx| {
        if let Some(app) = crate::model::try_shared(cx) {
            reveal_log_file(&app, cx);
        }
    });
}

/// Items for a Help menu. PR #68 can append these when it creates menus.
#[allow(dead_code)]
pub fn help_menu_items() -> Vec<MenuItem> {
    vec![
        MenuItem::action("Copy Logs", CopyLogs),
        MenuItem::action("Reveal Log File", RevealLogFile),
    ]
}

actions!(convt_support, [CopyLogs, RevealLogFile]);
