//! The document pack in the windows: the card Quick convert shows for
//! documents nothing can convert yet, and the Documents row in Settings.
//!
//! A click is the only way to start a download: the Download button
//! ([`download_button`]) or Yes on onboarding's documents question, both
//! through [`start_install`], the one caller of `AppState::download_pack`.
//! Opening a window or showing a card never does.

use super::theme::IconName;
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

use super::theme::{self, Button, Palette, Tone, mono, radius, size, space, styled};
use super::{error_text, human_size};
use crate::model::{AppState, PackPhase, PackState};
use crate::pack::{self, FailureKind, Progress, Status};

/// "Download (150 MB)", with the size pinned into this build when it has one.
fn download_label(verb: &str, offer: &pack::Offer) -> String {
    match offer.download {
        Some(bytes) => format!("{verb} ({})", human_size(bytes)),
        None => verb.to_string(),
    }
}

/// Starts downloading and installing the pack. Call it only from a click
/// that asks for it.
pub(super) fn start_install(app: &Entity<AppState>, cx: &mut App) {
    app.update(cx, |s, cx| s.download_pack(cx))
}

/// The button that starts the download.
fn download_button(app: &Entity<AppState>, label: String, p: &Palette) -> theme::Clickable {
    let app = app.clone();
    Button::primary("pack-download", label)
        .icon(IconName::Download)
        .build(p)
        .on_click(move |_, _, cx| start_install(&app, cx))
}

fn cancel_button(app: &Entity<AppState>, p: &Palette) -> theme::Clickable {
    let app = app.clone();
    Button::secondary("pack-cancel", "Cancel download")
        .small()
        .build(p)
        .on_click(move |_, _, cx| app.update(cx, |s, _| s.cancel_pack_download()))
}

/// The progress bar and its caption while the pack installs.
fn progress(progress: &Progress, offer: &pack::Offer, p: &Palette) -> Div {
    let (fraction, caption) = match progress {
        Progress::Download { bytes, total } => {
            // A file source reports no useful total; the pinned size stands in.
            let total = total.filter(|t| *t >= *bytes && *t > 0).or(offer.download);
            match total {
                Some(total) => (
                    *bytes as f32 / total as f32,
                    format!("{} of {}", human_size(*bytes), human_size(total)),
                ),
                None => (0., human_size(*bytes)),
            }
        }
        Progress::Verifying => (1., "Checking the download…".to_string()),
        Progress::Installing => (1., "Installing…".to_string()),
    };
    div()
        .flex()
        .flex_col()
        .gap(px(6.))
        .child(theme::progress(fraction, p.track, p.green))
        .child(
            div()
                .id("pack-progress")
                .test_support()
                .aria_label(SharedString::from(caption.clone()))
                .child(mono(11., 14., p.secondary).child(caption)),
        )
}

/// "150 MB download · about 410 MB on disk".
fn sizes_line(offer: &pack::Offer) -> String {
    let mut parts = Vec::new();
    if let Some(bytes) = offer.download {
        parts.push(format!("{} download", human_size(bytes)));
    }
    if let Some(bytes) = offer.installed {
        parts.push(format!("about {} on disk", human_size(bytes)));
    }
    parts.join(" · ")
}

/// Where the pack goes, on its own line: the home folder shortened to `~`,
/// cut from the start so the last folders stay readable, and the whole
/// path on hover.
fn destination_line(offer: &pack::Offer, p: &Palette) -> Option<impl IntoElement + use<>> {
    let path = SharedString::from(super::tilde(offer.destination.as_ref()?));
    let hover = path.clone();
    Some(
        div()
            .id("pack-destination")
            .test_support()
            .aria_label(path.clone())
            .tooltip(move |window, cx| Tooltip::new(hover.clone()).build(window, cx))
            .child(
                mono(11., 14., p.tertiary)
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis_start()
                    .child(path),
            ),
    )
}

/// What the card says: title, body and the detail under it.
struct Words {
    title: String,
    body: String,
    detail: Option<String>,
}

fn words(pack: &PackState, mixed: bool) -> Words {
    let skipped = if mixed {
        " Until then, documents are skipped."
    } else {
        ""
    };
    if pack.removing {
        return Words {
            title: "Removing document support".into(),
            body: "Documents can't convert until it's downloaded again.".into(),
            detail: None,
        };
    }
    match (&pack.phase, &pack.status) {
        (PackPhase::Working(Progress::Download { .. }), _) => Words {
            title: "Downloading document support".into(),
            body: "If you cancel, convt keeps what it has so far.".into(),
            detail: None,
        },
        (PackPhase::Working(_), _) => Words {
            title: "Installing document support".into(),
            body: "Checking the download against the checksum built into convt.".into(),
            detail: None,
        },
        (PackPhase::Failed(f), _) if f.kind != FailureKind::Cancelled => {
            let (title, body) = pack::plain_failure(f, &pack.offer);
            Words {
                title,
                body,
                detail: Some(f.message.clone()),
            }
        }
        (_, Status::Rejected(reason)) => Words {
            title: "Document support needs reinstalling".into(),
            body: format!(
                "{} Download it again to fix this.{skipped}",
                pack::plain_reason(reason)
            ),
            detail: Some(reason.clone()),
        },
        _ if !pack.offer.configured => Words {
            title: "Document support isn't installed".into(),
            body: format!(
                "This build of convt has no document pack to download. If LibreOffice is installed on {}, convt uses it.",
                theme::this_machine()
            ),
            detail: None,
        },
        (phase, _) => Words {
            title: "Document support isn't installed".into(),
            body: format!(
                "Word, Excel, PowerPoint and OpenDocument files convert with LibreOffice, which convt downloads once. After that, documents convert offline on {}, like everything else.{skipped}",
                theme::this_machine()
            ),
            detail: matches!(phase, PackPhase::Failed(f) if f.kind == FailureKind::Cancelled)
                .then(|| "Download stopped. Downloading again picks up where it stopped.".into()),
        },
    }
}

/// The action under the words: Download, Try again, or progress with Cancel.
fn action(app: &Entity<AppState>, pack: &PackState, p: &Palette) -> Option<AnyElement> {
    match &pack.phase {
        PackPhase::Working(step) => {
            // The engines stop until extraction starts.
            let cancel = (!matches!(step, Progress::Installing)).then(|| cancel_button(app, p));
            Some(
                div()
                    .flex()
                    .items_center()
                    .gap(px(space::MD))
                    .child(div().flex_1().child(progress(step, &pack.offer, p)))
                    .children(cancel)
                    .into_any_element(),
            )
        }
        _ if !pack.offer.configured || pack.removing => None,
        phase => {
            let verb = match (phase, &pack.status) {
                (PackPhase::Failed(f), _) if f.kind != FailureKind::Cancelled => "Try again",
                (_, Status::Rejected(_)) => "Download again",
                _ => "Download",
            };
            Some(
                div()
                    .flex()
                    .child(download_button(app, download_label(verb, &pack.offer), p))
                    .into_any_element(),
            )
        }
    }
}

/// A document page with its badge, the card's icon.
fn page_icon(p: &Palette) -> Div {
    div()
        .flex()
        .flex_shrink_0()
        .items_end()
        .justify_center()
        .w(px(36.))
        .h(px(44.))
        .pb(px(8.))
        .rounded(px(radius::SM))
        .bg(p.thumb)
        .shadow(vec![theme::inset_ring(p.thumb_border, 1.)])
        .child(
            div()
                .px(px(3.))
                .py(px(1.))
                .rounded(px(2.))
                .bg(rgb(0x2B5BB8))
                .child(
                    mono(8., 10., rgb(0xFFFFFF).into())
                        .font_weight(FontWeight::SEMIBOLD)
                        .child("DOC"),
                ),
        )
}

/// Quick convert's card for documents nothing can convert yet. `mixed`:
/// other files in the selection can convert now.
pub(super) fn card(
    app: &Entity<AppState>,
    mixed: bool,
    p: &Palette,
    cx: &App,
) -> impl IntoElement + use<> {
    let pack = &app.read(cx).pack;
    let words = words(pack, mixed);
    let idle = !matches!(pack.phase, PackPhase::Working(_));
    let sizes = sizes_line(&pack.offer);
    div()
        .id("pack-card")
        .test_support()
        .aria_label(SharedString::from(words.title.clone()))
        .flex()
        .gap(px(14.))
        .p(px(18.))
        .rounded(px(radius::PANEL))
        .bg(p.surface)
        .shadow({
            let mut s = vec![theme::inset_ring(p.border, 1.)];
            s.extend(theme::soft(p));
            s
        })
        .child(page_icon(p))
        .child(
            div()
                .flex()
                .flex_col()
                .flex_1()
                .min_w_0()
                .gap(px(6.))
                .child(
                    styled(size::BODY, p.text)
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(words.title),
                )
                .child(
                    div()
                        .id("pack-body")
                        .test_support()
                        .aria_label(SharedString::from(words.body.clone()))
                        .child(styled(size::SMALL, p.secondary).child(words.body)),
                )
                .when(idle && pack.offer.configured && !sizes.is_empty(), |d| {
                    d.child(mono(11., 14., p.tertiary).truncate().child(sizes))
                })
                .when(idle && pack.offer.configured, |d| {
                    d.children(destination_line(&pack.offer, p))
                })
                .children(words.detail.map(|detail| {
                    div()
                        .id("pack-detail")
                        .test_support()
                        .aria_label(SharedString::from(detail.clone()))
                        .child(mono(11., 14., p.tertiary).child(detail))
                }))
                .children(pack.notice.clone().map(|e| error_text(e, p)))
                .children(action(app, pack, p).map(|a| div().pt(px(6.)).child(a))),
        )
}

/// The line Quick convert shows once the pack it offered is installed.
pub(super) fn installed_notice(p: &Palette) -> impl IntoElement + use<> {
    div()
        .id("pack-done")
        .test_support()
        .aria_label("Document support is installed. Pick a format to convert.")
        .flex()
        .items_center()
        .gap(px(6.))
        .child(theme::icon(IconName::CircleCheck, 14., p.green_text))
        .child(
            styled(size::SMALL, p.green_text)
                .font_weight(FontWeight::MEDIUM)
                .child("Document support is installed. Pick a format to convert."),
        )
}

/// Activity's line for a document download that runs in the background,
/// such as one onboarding started: its progress, then a plain failure with
/// the way to try again. Nothing once it's installed or never started.
pub(super) fn activity_notice(pack: &PackState, p: &Palette) -> Option<AnyElement> {
    match &pack.phase {
        PackPhase::Working(step) => Some(
            div()
                .id("activity-pack")
                .test_support()
                .aria_label("Adding document support")
                .child(theme::callout(
                    IconName::Document,
                    Tone::Neutral,
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(space::SM))
                        .child(theme::callout_words(
                            "Adding document support",
                            "Word, Excel and PowerPoint files convert once it's done.",
                            p,
                        ))
                        .child(progress(step, &pack.offer, p)),
                    p,
                ))
                .into_any_element(),
        ),
        PackPhase::Failed(f) if f.kind != FailureKind::Cancelled => {
            let (title, body) = pack::plain_failure(f, &pack.offer);
            Some(
                div()
                    .id("activity-pack")
                    .test_support()
                    .aria_label(SharedString::from(title.clone()))
                    .child(theme::callout(
                        IconName::TriangleAlert,
                        Tone::Error,
                        div()
                            .flex()
                            .items_center()
                            .gap(px(space::LG))
                            .child(theme::callout_words(title, body, p).flex_1().min_w_0())
                            .child(
                                Button::secondary(
                                    "activity-pack-settings",
                                    "Try again in Settings",
                                )
                                .small()
                                .build(p)
                                .on_click(|_, _, cx| {
                                    super::show_settings(super::SettingsTab::General, cx)
                                }),
                            ),
                        p,
                    ))
                    .into_any_element(),
            )
        }
        _ => None,
    }
}

/// A click in the Remove flow.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Remove {
    Ask,
    Confirm,
    Keep,
}

/// The version of the pack this build installs, when its build pinned one.
fn installed_version() -> Option<String> {
    let version = convt_engines::packs::documents_source().version;
    (version != "unconfigured").then_some(version)
}

/// The Documents row in Settings: what is installed, Install or Uninstall.
/// `confirming`: the user clicked Remove once and is asked again.
pub(super) fn settings_row(
    app: &Entity<AppState>,
    confirming: bool,
    on_remove: impl Fn(Remove, &mut Window, &mut App) + Clone + 'static,
    p: &Palette,
    cx: &App,
) -> Div {
    let state = app.read(cx);
    let pack = &state.pack;
    let system = state.documents_supported() && !matches!(pack.status, Status::Installed(_));
    let (summary, color) = match (&pack.phase, &pack.status) {
        _ if pack.removing => ("Removing…".to_string(), p.secondary),
        (PackPhase::Working(Progress::Download { .. }), _) => {
            ("Downloading…".to_string(), p.secondary)
        }
        (PackPhase::Working(_), _) => ("Installing…".to_string(), p.secondary),
        (PackPhase::Failed(f), _) if f.kind != FailureKind::Cancelled => {
            (pack::plain_failure(f, &pack.offer).0, p.error)
        }
        (_, Status::Installed(_)) => (
            match installed_version() {
                Some(version) => format!("Installed · version {version}"),
                None => "Installed".to_string(),
            },
            p.green_text,
        ),
        (_, Status::Rejected(reason)) => (pack::plain_reason(reason).to_string(), p.error),
        _ if system => (
            format!("Using LibreOffice on {}", theme::this_machine()),
            p.secondary,
        ),
        _ if !pack.offer.configured => (
            "Not installed. This build has no pack to download.".to_string(),
            p.secondary,
        ),
        _ => ("Not installed".to_string(), p.secondary),
    };
    let status = div()
        .id("pack-status")
        .test_support()
        .aria_label(SharedString::from(summary.clone()))
        .child(styled(size::SMALL, color).child(summary));
    let installed = matches!(pack.status, Status::Installed(_) | Status::Rejected(_));
    let idle = !matches!(pack.phase, PackPhase::Working(_)) && !pack.removing;
    let ask = on_remove.clone();
    let remove = (installed && idle && !confirming).then(|| {
        Button::secondary("pack-remove", "Uninstall")
            .small()
            .build(p)
            .on_click(move |_, window, cx| ask(Remove::Ask, window, cx))
            .into_any_element()
    });
    let confirm = (installed && idle && confirming).then(|| {
        let (yes, no) = (on_remove.clone(), on_remove.clone());
        theme::callout(
            IconName::TriangleAlert,
            Tone::Error,
            div()
                .flex()
                .flex_col()
                .gap(px(space::SM))
                .child(theme::callout_words(
                    "Uninstall document support?",
                    "Documents won't convert until you download it again.",
                    p,
                ))
                .child(
                    div()
                        .flex()
                        .gap(px(space::SM))
                        .child(
                            Button::secondary("pack-remove-confirm", "Uninstall")
                                .color(p.error)
                                .small()
                                .build(p)
                                .on_click(move |_, window, cx| yes(Remove::Confirm, window, cx)),
                        )
                        .child(
                            Button::ghost("pack-remove-keep", "Keep")
                                .small()
                                .build(p)
                                .on_click(move |_, window, cx| no(Remove::Keep, window, cx)),
                        ),
                ),
            p,
        )
    });
    // Offer the download where it would help: nothing installed and no
    // LibreOffice on this computer, a failed try, or a rejected pack.
    // While it works, the progress goes under the row.
    let offer = matches!(pack.phase, PackPhase::Working(_))
        .then(|| action(app, pack, p))
        .flatten();
    // Idle, Install sits on the right, where Uninstall does.
    let install = match (&pack.phase, &pack.status) {
        (PackPhase::Working(_), _) | (_, Status::Installed(_)) => None,
        (_, Status::NotInstalled) if system => None,
        _ if !pack.offer.configured || pack.removing => None,
        (phase, status) => {
            let verb = match (phase, status) {
                (PackPhase::Failed(f), _) if f.kind != FailureKind::Cancelled => "Try again",
                (_, Status::Rejected(_)) => "Reinstall",
                _ => "Install",
            };
            Some(download_button(app, download_label(verb, &pack.offer), p).into_any_element())
        }
    };
    let detail = match (&pack.phase, &pack.status) {
        (PackPhase::Failed(f), _) if f.kind != FailureKind::Cancelled => Some(f.message.clone()),
        (_, Status::Rejected(reason)) => Some(reason.clone()),
        _ => None,
    };
    div()
        .flex()
        .flex_col()
        .gap(px(space::SM))
        .px(px(space::LG))
        .py(px(space::MD))
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(space::LG))
                .min_h(px(30.))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .flex_1()
                        .min_w_0()
                        .gap(px(2.))
                        .child(
                            styled(size::BODY, p.text)
                                .font_weight(FontWeight::MEDIUM)
                                .child("Document support"),
                        )
                        .child(status),
                )
                .children(remove)
                .children(install),
        )
        .children(detail.map(|d| {
            div()
                .id("pack-detail")
                .test_support()
                .aria_label(SharedString::from(d.clone()))
                .child(mono(11., 14., p.tertiary).child(d))
        }))
        .children(offer)
        .children(confirm)
        .children(pack.notice.clone().map(|e| error_text(e, p)))
}
