//! The document pack in the windows: the card Quick convert shows for
//! documents nothing can convert yet or a target that needs the pack, the
//! card on Activity, and the Documents row in Settings.
//!
//! A click is the only way to start a download: [`download_button`] builds
//! every button that does (Download, Try again, Install & Convert), and its
//! click handler is the one caller of `AppState::download_pack`. Opening a
//! window or showing a card never does.

use std::rc::Rc;

use convt_core::format_by_id;

use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

use super::theme::{self, Palette, mono, primary_button, secondary_button, text, text_button};
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

/// What a button does after it started the download, such as Quick convert
/// remembering to convert once the pack is installed.
pub(super) type After = Rc<dyn Fn(&mut Window, &mut App)>;

/// The button that starts the download, the only caller of
/// `AppState::download_pack`. A download already running is left alone,
/// and `after` still runs, so a second click can join it.
fn download_button(
    id: &'static str,
    app: &Entity<AppState>,
    label: String,
    disabled: bool,
    after: Option<After>,
) -> theme::Clickable {
    let app = app.clone();
    primary_button(id, label, 13., disabled).on_click(move |_, window, cx| {
        if disabled {
            return;
        }
        app.update(cx, |s, cx| s.download_pack(cx));
        if let Some(after) = &after {
            after(window, cx);
        }
    })
}

/// Quick convert's main button for a target that needs the pack: it starts
/// the download (or joins one already running) and then `after`.
pub(super) fn install_and_convert_button(
    app: &Entity<AppState>,
    label: String,
    disabled: bool,
    after: After,
) -> theme::Clickable {
    download_button("convert", app, label, disabled, Some(after))
}

/// The marker on a target card that needs the pack.
pub(super) fn tag(id: SharedString, on: bool, p: &Palette) -> impl IntoElement + use<> {
    div()
        .id(id)
        .test_support()
        .aria_label("Document pack")
        .flex()
        .child(
            div()
                .px(px(4.))
                .rounded(px(3.))
                .bg(if on { p.window } else { p.chip })
                .border_1()
                .border_color(p.chip_border)
                .child(
                    text(10., 13., p.secondary)
                        .font_weight(FontWeight::MEDIUM)
                        .whitespace_nowrap()
                        .child("Document pack"),
                ),
        )
}

/// What the pack adds and what works without it, from the registries, so
/// the words never promise a conversion this build doesn't have.
pub(super) fn what_it_adds(state: &AppState) -> String {
    let f = |id| format_by_id(id).expect("a known format");
    let (pdf, png, txt, docx) = (f("pdf"), f("png"), f("txt"), f("docx"));
    let mut adds = "Adds Word, Excel, PowerPoint and OpenDocument files".to_string();
    if state.pack_targets(pdf).contains(&docx) {
        adds.push_str(", and PDF to Word");
    }
    adds.push('.');
    let now = |from, to| state.registry.targets(from).contains(&to);
    let free: Vec<&str> = [
        (now(png, pdf), "images to PDF"),
        (now(pdf, png), "PDF to images"),
        (now(pdf, txt), "PDF to text"),
    ]
    .into_iter()
    .filter_map(|(works, what)| works.then_some(what))
    .collect();
    let free = match free.as_slice() {
        [] => return adds,
        [one] => one.to_string(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    };
    let mut free = free;
    free[..1].make_ascii_uppercase();
    format!("{adds} {free} already work without it.")
}

fn cancel_button(app: &Entity<AppState>, p: &Palette) -> theme::Clickable {
    let app = app.clone();
    secondary_button("pack-cancel", "Cancel download", p)
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

/// Why Quick convert shows the card.
pub(super) enum Need {
    /// Files only the pack can convert. `mixed`: other files in the
    /// selection convert now.
    Files { mixed: bool },
    /// The picked target needs the pack, though the files open without it:
    /// `from` names them ("this PDF"), `to` the target.
    Target { from: String, to: &'static str },
}

fn words(pack: &PackState, need: &Need) -> Words {
    let skipped = if matches!(need, Need::Files { mixed: true }) {
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
            body: match need {
                Need::Target { from, to } => format!(
                    "Turning {from} into {to} uses LibreOffice, which convt downloads once. After that, documents convert offline on {}, like everything else.",
                    theme::this_machine()
                ),
                Need::Files { .. } => format!(
                    "Word, Excel, PowerPoint and OpenDocument files convert with LibreOffice, which convt downloads once. After that, documents convert offline on {}, like everything else.{skipped}",
                    theme::this_machine()
                ),
            },
            detail: matches!(phase, PackPhase::Failed(f) if f.kind == FailureKind::Cancelled)
                .then(|| "Download stopped. Downloading again picks up where it stopped.".into()),
        },
    }
}

/// The action under the words: Download, Try again, or progress with Cancel.
/// Without `download`, an idle card has no button, because the window's own
/// Install & Convert starts the download.
fn action(
    app: &Entity<AppState>,
    pack: &PackState,
    download: bool,
    p: &Palette,
) -> Option<AnyElement> {
    match &pack.phase {
        PackPhase::Working(step) => {
            // The engines stop until extraction starts.
            let cancel = (!matches!(step, Progress::Installing)).then(|| cancel_button(app, p));
            Some(
                div()
                    .flex()
                    .items_center()
                    .gap(px(14.))
                    .child(div().flex_1().child(progress(step, &pack.offer, p)))
                    .children(cancel)
                    .into_any_element(),
            )
        }
        _ if !pack.offer.configured || pack.removing || !download => None,
        phase => {
            let verb = match (phase, &pack.status) {
                (PackPhase::Failed(f), _) if f.kind != FailureKind::Cancelled => "Try again",
                (_, Status::Rejected(_)) => "Download again",
                _ => "Download",
            };
            Some(
                div()
                    .flex()
                    .child(download_button(
                        "pack-download",
                        app,
                        download_label(verb, &pack.offer),
                        false,
                        None,
                    ))
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
        .w(px(38.))
        .h(px(46.))
        .pb(px(8.))
        .rounded(px(5.))
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

/// Quick convert's card for documents nothing can convert yet, or a target
/// that needs the pack. `download`: the card carries the Download button;
/// without it the window's Install & Convert is the one action.
pub(super) fn card(
    app: &Entity<AppState>,
    need: &Need,
    download: bool,
    p: &Palette,
    cx: &App,
) -> impl IntoElement + use<> {
    let pack = &app.read(cx).pack;
    let words = words(pack, need);
    let idle = !matches!(pack.phase, PackPhase::Working(_));
    let sizes = sizes_line(&pack.offer);
    div()
        .id("pack-card")
        .test_support()
        .aria_label(SharedString::from(words.title.clone()))
        .flex()
        .gap(px(14.))
        .p(px(16.))
        .rounded(px(8.))
        .bg(p.recessed)
        .border_1()
        .border_color(p.recessed_border)
        .child(page_icon(p))
        .child(
            div()
                .flex()
                .flex_col()
                .flex_1()
                .min_w_0()
                .gap(px(6.))
                .child(
                    text(13., 16., p.text)
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(words.title),
                )
                .child(
                    div()
                        .id("pack-body")
                        .test_support()
                        .aria_label(SharedString::from(words.body.clone()))
                        .child(text(12., 17., p.secondary).child(words.body)),
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
                .children(action(app, pack, download, p).map(|a| div().pt(px(6.)).child(a))),
        )
}

/// The line Quick convert shows once the pack it offered is installed.
/// `picked`: a target is already picked, so Convert is what's left.
pub(super) fn installed_notice(picked: bool, p: &Palette) -> impl IntoElement + use<> {
    let words = if picked {
        "Document support is installed. Click Convert to finish."
    } else {
        "Document support is installed. Pick a format to convert."
    };
    div()
        .id("pack-done")
        .test_support()
        .aria_label(words)
        .child(
            text(12., 16., p.green)
                .font_weight(FontWeight::MEDIUM)
                .child(words),
        )
}

/// A click in the Remove flow.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Remove {
    Ask,
    Confirm,
    Keep,
}

/// The Documents row in Settings: what is installed, Download or Remove.
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
        (_, Status::Installed(_)) => ("Installed".to_string(), p.green),
        (_, Status::Rejected(reason)) => (pack::plain_reason(reason).to_string(), p.error),
        _ if system => (
            format!("Using LibreOffice on {}", theme::this_machine()),
            p.text,
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
        .child(text(13., 16., color).child(summary));
    let installed = matches!(pack.status, Status::Installed(_) | Status::Rejected(_));
    let idle = !matches!(pack.phase, PackPhase::Working(_)) && !pack.removing;
    let remove = (installed && idle).then(|| {
        if confirming {
            let (yes, no) = (on_remove.clone(), on_remove.clone());
            div()
                .flex()
                .flex_col()
                .gap(px(8.))
                .child(text(12., 16., p.secondary).child(
                    "Remove document support? Documents won't convert until you download it again.",
                ))
                .child(
                    div()
                        .flex()
                        .gap(px(14.))
                        .child(
                            text_button("pack-remove-confirm", "Remove", p.error, 12.)
                                .on_click(move |_, window, cx| yes(Remove::Confirm, window, cx)),
                        )
                        .child(
                            text_button("pack-remove-keep", "Keep", p.text, 12.)
                                .on_click(move |_, window, cx| no(Remove::Keep, window, cx)),
                        ),
                )
                .into_any_element()
        } else {
            let ask = on_remove.clone();
            text_button("pack-remove", "Remove", p.secondary, 12.)
                .on_click(move |_, window, cx| ask(Remove::Ask, window, cx))
                .into_any_element()
        }
    });
    // Offer the download where it would help: nothing installed and no
    // LibreOffice on this computer, a failed try, or a rejected pack.
    let offer = match (&pack.phase, &pack.status) {
        (PackPhase::Working(_), _) => action(app, pack, true, p),
        (_, Status::Installed(_)) => None,
        (_, Status::NotInstalled) if system => None,
        _ => action(app, pack, true, p),
    };
    // What the pack is for, while it would still add something.
    let about = (pack.offer.configured && !system && !installed_status(&pack.status)).then(|| {
        let adds = what_it_adds(state);
        div()
            .id("pack-about")
            .test_support()
            .aria_label(SharedString::from(adds.clone()))
            .child(text(12., 16., p.secondary).child(adds))
    });
    let detail = match (&pack.phase, &pack.status) {
        (PackPhase::Failed(f), _) if f.kind != FailureKind::Cancelled => Some(f.message.clone()),
        (_, Status::Rejected(reason)) => Some(reason.clone()),
        _ => None,
    };
    div()
        .flex()
        .flex_col()
        .gap(px(8.))
        .w(px(330.))
        .child(status)
        .children(about)
        .children(detail.map(|d| {
            div()
                .id("pack-detail")
                .test_support()
                .aria_label(SharedString::from(d.clone()))
                .child(mono(11., 14., p.tertiary).child(d))
        }))
        .children(offer)
        .children(remove)
        .children(pack.notice.clone().map(|e| error_text(e, p)))
}

fn installed_status(status: &Status) -> bool {
    matches!(status, Status::Installed(_))
}

/// The card on Activity that points at the document pack, for people who
/// finished setup before it existed or never opened a document. It shows
/// while the pack would add something and the user hasn't dismissed it,
/// and follows a download started anywhere.
pub(super) fn activity_card(
    app: &Entity<AppState>,
    on_dismiss: impl Fn(&mut Window, &mut App) + 'static,
    p: &Palette,
    cx: &App,
) -> Option<AnyElement> {
    let state = app.read(cx);
    let pack = &state.pack;
    let working = matches!(pack.phase, PackPhase::Working(_));
    if state.settings.pack_hint_dismissed && !working {
        return None;
    }
    let done = pack.phase == PackPhase::Done && installed_status(&pack.status);
    if !(state.pack_worth_offering() || working || done) || pack.removing {
        return None;
    }
    let idle = matches!(pack.phase, PackPhase::Idle)
        || matches!(&pack.phase, PackPhase::Failed(f) if f.kind == FailureKind::Cancelled);
    let (title, body, detail) = if done {
        (
            "Document support is installed".to_string(),
            format!(
                "Word, Excel, PowerPoint and OpenDocument files now convert on {}. Drop one here or right-click it.",
                theme::this_machine()
            ),
            None,
        )
    } else if idle && !matches!(pack.status, Status::Rejected(_)) {
        (
            "Convert Word, Excel and PowerPoint files".to_string(),
            format!(
                "{} It's a one-time download; after that, documents convert offline.",
                what_it_adds(state)
            ),
            None,
        )
    } else {
        let words = words(pack, &Need::Files { mixed: false });
        (words.title, words.body, words.detail)
    };
    let sizes = sizes_line(&pack.offer);
    let dismiss = (!working).then(|| {
        text_button(
            "pack-hint-dismiss",
            if done { "Dismiss" } else { "Not now" },
            p.secondary,
            12.,
        )
        .on_click(move |_, window, cx| on_dismiss(window, cx))
    });
    let action = (!done).then(|| action(app, pack, true, p)).flatten();
    Some(
        div()
            .id("pack-hint")
            .test_support()
            .aria_label(SharedString::from(title.clone()))
            .flex()
            .gap(px(14.))
            .mb(px(8.))
            .p(px(14.))
            .rounded(px(8.))
            .bg(p.recessed)
            .border_1()
            .border_color(p.recessed_border)
            .child(page_icon(p))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_w_0()
                    .gap(px(6.))
                    .child(
                        text(13., 16., p.text)
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(title),
                    )
                    .child(
                        div()
                            .id("pack-hint-body")
                            .test_support()
                            .aria_label(SharedString::from(body.clone()))
                            .child(text(12., 17., p.secondary).child(body)),
                    )
                    .when(idle && !sizes.is_empty(), |d| {
                        d.child(mono(11., 14., p.tertiary).truncate().child(sizes))
                    })
                    .children(detail.map(|detail| {
                        div()
                            .id("pack-detail")
                            .test_support()
                            .aria_label(SharedString::from(detail.clone()))
                            .child(mono(11., 14., p.tertiary).child(detail))
                    }))
                    .children(pack.notice.clone().map(|e| error_text(e, p)))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(14.))
                            .pt(px(4.))
                            .children(action.map(|a| div().flex_1().child(a)))
                            .children(dismiss),
                    ),
            )
            .into_any_element(),
    )
}
