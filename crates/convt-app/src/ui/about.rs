//! About convt: the mark, this version and its build date, and links to the
//! site, the release notes and the source code (the app is AGPL).

use gpui_kit::*;

use super::menus::SOURCE_URL;
use super::theme::{self, Button, size, space, styled};
use super::update::{long_date, release_notes_url};
use crate::account::VERSION;
use crate::model::AppState;

/// The About window's size.
pub(super) const ABOUT_SIZE: (f32, f32) = (380., 400.);

pub struct AboutView {
    app: Entity<AppState>,
    _appearance: Subscription,
}

impl AboutView {
    pub fn new(app: Entity<AppState>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        Self {
            app,
            _appearance: theme::observe_appearance(window, cx),
        }
    }
}

impl Render for AboutView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = theme::palette(cx);
        let version = format!(
            "Version {VERSION} · built {}",
            long_date(self.app.read(cx).licensing.build_date())
        );
        let link = |id: &'static str, label: &'static str, url: String| {
            Button::ghost(id, label)
                .small()
                .build(&p)
                .on_click(move |_, _, cx| cx.open_url(&url))
        };
        div()
            .id("about")
            .flex()
            .flex_col()
            .size_full()
            .bg(p.window)
            .font_family(theme::SANS)
            .text_color(p.text)
            .children(theme::title_bar(None, 32., None, &p))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .items_center()
                    .justify_center()
                    .gap(px(space::SM))
                    .px(px(space::XL))
                    .pb(px(space::XL))
                    .child(theme::mark(56., &p))
                    .child(
                        styled(size::DISPLAY, p.text)
                            .pt(px(space::SM))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child("convt"),
                    )
                    .child(
                        div()
                            .id("about-version")
                            .test_support()
                            .aria_label(SharedString::from(version.clone()))
                            .child(theme::mono(11.5, 16., p.secondary).child(version)),
                    )
                    .child(
                        styled(size::SMALL, p.secondary)
                            .text_center()
                            .child("Convert files without uploading them."),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_wrap()
                            .justify_center()
                            .gap(px(space::XS))
                            .pt(px(space::MD))
                            .child(link("about-site", "convt.app", "https://convt.app".into()))
                            .child(link(
                                "about-notes",
                                "Release notes",
                                release_notes_url(VERSION),
                            ))
                            .child(link("about-source", "Source code", SOURCE_URL.into())),
                    )
                    .child(
                        styled(size::CAPTION, p.tertiary)
                            .pt(px(space::SM))
                            .child("Free software under the GNU AGPL v3"),
                    ),
            )
    }
}
