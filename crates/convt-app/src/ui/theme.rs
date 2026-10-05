//! The convt look: the light and dark palettes from the design, the bundled
//! Geist fonts, and the small controls every window shares. The theme follows
//! the system appearance.

use std::borrow::Cow;

use gpui_kit::component::input::{Input, InputState};
use gpui_kit::component::{ActiveTheme, Icon, IconName, Sizable, Theme};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

pub const SANS: &str = "Geist";
pub const MONO: &str = "Geist Mono";

/// Geist and Geist Mono 1.7.2, under the SIL Open Font License 1.1
/// (`assets/fonts/OFL.txt`).
const FONTS: [&[u8]; 6] = [
    include_bytes!("../../assets/fonts/Geist-Regular.ttf"),
    include_bytes!("../../assets/fonts/Geist-Medium.ttf"),
    include_bytes!("../../assets/fonts/Geist-SemiBold.ttf"),
    include_bytes!("../../assets/fonts/GeistMono-Regular.ttf"),
    include_bytes!("../../assets/fonts/GeistMono-Medium.ttf"),
    include_bytes!("../../assets/fonts/GeistMono-SemiBold.ttf"),
];

/// Every color the windows use, for one appearance.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Palette {
    pub dark: bool,
    pub window: Hsla,
    /// Sidebar, settings toolbar and the dark title bars.
    pub chrome: Hsla,
    pub chrome_border: Hsla,
    pub nav_selected: Hsla,
    pub tab_selected: Hsla,
    pub text: Hsla,
    pub secondary: Hsla,
    pub tertiary: Hsla,
    /// Section dividers.
    pub hairline: Hsla,
    /// Dividers between list rows.
    pub row_divider: Hsla,
    pub track: Hsla,
    pub green: Hsla,
    pub green_tint: Hsla,
    pub error: Hsla,
    /// The defaults bar, footers and other recessed areas.
    pub recessed: Hsla,
    pub recessed_border: Hsla,
    pub chip: Hsla,
    pub chip_border: Hsla,
    pub card_border: Hsla,
    pub control: Hsla,
    pub control_border: Hsla,
    pub segmented: Hsla,
    pub segmented_selected: Hsla,
    pub mark_off: Hsla,
    pub radio_off: Hsla,
    pub toggle_off: Hsla,
    /// The checked fill of checkboxes, radios and switches. The design keeps
    /// the light green in both appearances.
    pub control_on: Hsla,
    pub step_off: Hsla,
    pub thumb: Hsla,
    pub thumb_border: Hsla,
    pub trial_card: Hsla,
    pub popover: Hsla,
    pub drop_bar: Hsla,
    pub drop_border: Hsla,
    pub popover_footer: Hsla,
    pub popover_hairline: Hsla,
    pub popover_track: Hsla,
    pub mock_item: Hsla,
    pub mock_menu: Hsla,
    pub mock_separator: Hsla,
    pub shadow: Hsla,
    pub shadow_soft: Hsla,
}

fn c(hex: u32) -> Hsla {
    rgb(hex).into()
}

fn ca(hex: u32) -> Hsla {
    rgba(hex).into()
}

impl Palette {
    pub fn light() -> Self {
        Self {
            dark: false,
            window: c(0xFFFFFF),
            chrome: c(0xF3F4F3),
            chrome_border: c(0xE3E5E4),
            nav_selected: c(0xE3E6E4),
            tab_selected: c(0xE1E4E2),
            text: c(0x0A0A0A),
            secondary: c(0x6B6F6D),
            tertiary: c(0x8A8F8C),
            hairline: c(0xE6E8E7),
            row_divider: c(0xEEF0EF),
            track: c(0xE6E8E7),
            green: c(0x1A9A5B),
            green_tint: c(0xEEF7F2),
            error: c(0xB3261E),
            recessed: c(0xF7F8F7),
            recessed_border: c(0xE6E8E7),
            chip: c(0xFFFFFF),
            chip_border: c(0xE1E4E2),
            card_border: c(0xE0E3E1),
            control: c(0xFFFFFF),
            control_border: c(0xDCDFDD),
            segmented: c(0xF0F2F1),
            segmented_selected: c(0xFFFFFF),
            mark_off: c(0xBFC4C1),
            radio_off: c(0xC9CDCB),
            toggle_off: c(0xD5D9D7),
            control_on: c(0x1A9A5B),
            step_off: c(0xE4E7E5),
            thumb: c(0xFFFFFF),
            thumb_border: c(0xD9DCDA),
            trial_card: c(0xFFFFFF),
            popover: ca(0xFAFAFAFB),
            drop_bar: c(0xFFFFFF),
            drop_border: c(0xC9CDCB),
            popover_footer: c(0xF4F5F4),
            popover_hairline: c(0xE6E8E7),
            popover_track: c(0xE4E7E5),
            mock_item: c(0xFFFFFF),
            mock_menu: c(0xFFFFFF),
            mock_separator: c(0xE6E8E7),
            shadow: ca(0x0A1E1447),
            shadow_soft: ca(0x0000001F),
        }
    }

    pub fn dark() -> Self {
        Self {
            dark: true,
            window: c(0x1C1E1D),
            chrome: c(0x232625),
            chrome_border: c(0x2E3331),
            nav_selected: c(0x2E3331),
            tab_selected: c(0x2E3331),
            text: c(0xEDEFEE),
            secondary: c(0xA1A6A3),
            tertiary: c(0x868B88),
            hairline: c(0x2E3331),
            row_divider: c(0x232726),
            track: c(0x2E3331),
            green: c(0x3FCB84),
            green_tint: c(0x12261B),
            error: c(0xF2786D),
            recessed: c(0x161918),
            recessed_border: c(0x232726),
            chip: c(0x1C201E),
            chip_border: c(0x2E3331),
            card_border: c(0x2E3331),
            control: c(0x161918),
            control_border: c(0x2E3331),
            segmented: c(0x161918),
            segmented_selected: c(0x2E3331),
            mark_off: c(0x6F7572),
            radio_off: c(0x6F7572),
            toggle_off: c(0x4A4F4D),
            control_on: c(0x1A9A5B),
            step_off: c(0x2E3331),
            thumb: c(0x2A2D2C),
            thumb_border: ca(0xFFFFFF1A),
            trial_card: c(0x1C1E1D),
            popover: c(0x2A2D2C),
            drop_bar: c(0x232625),
            drop_border: ca(0xFFFFFF26),
            popover_footer: c(0x232625),
            popover_hairline: ca(0xFFFFFF1A),
            popover_track: ca(0xFFFFFF1A),
            mock_item: c(0x232625),
            mock_menu: c(0x2A2D2C),
            mock_separator: ca(0xFFFFFF1A),
            shadow: ca(0x00000099),
            shadow_soft: ca(0x00000066),
        }
    }
}

/// The palette for the current appearance.
pub fn palette(cx: &App) -> Palette {
    if cx.theme().is_dark() {
        Palette::dark()
    } else {
        Palette::light()
    }
}

/// Loads the bundled fonts and matches the theme to the system appearance.
pub fn init(cx: &mut App) {
    let fonts = FONTS.iter().map(|f| Cow::Borrowed(*f)).collect();
    if let Err(e) = cx.text_system().add_fonts(fonts) {
        tracing::warn!(error = %e, "could not load the Geist fonts");
    }
    crate::thumbs::init(cx);
    follow_system(None, cx);
}

/// Matches the theme to the system (or window) appearance and applies the
/// convt fonts and colors to the component library.
pub fn follow_system(window: Option<&mut Window>, cx: &mut App) {
    Theme::sync_system_appearance(window, cx);
    apply(cx);
}

/// Switches to a fixed appearance. Tests use it to render both themes.
#[cfg(test)]
pub fn set_dark(dark: bool, cx: &mut App) {
    use gpui_kit::component::ThemeMode;
    Theme::change(
        if dark {
            ThemeMode::Dark
        } else {
            ThemeMode::Light
        },
        None,
        cx,
    );
    apply(cx);
}

/// The component library draws text fields and scrollbars; give them the
/// convt fonts and colors.
fn apply(cx: &mut App) {
    let p = palette(cx);
    Theme::update(cx, |t| {
        t.font_family = SANS.into();
        t.mono_font_family = MONO.into();
        t.font_size = px(13.);
        t.mono_font_size = px(12.);
        t.radius = px(6.);
        t.background = p.window;
        t.foreground = p.text;
        t.muted_foreground = p.secondary;
        t.border = p.control_border;
        t.input = p.control_border;
        t.ring = p.green;
        t.primary = p.green;
        t.success = p.green;
        t.danger = p.error;
    });
}

/// Matches the theme to a new window's appearance and keeps it in step.
/// GPUI reads the appearance more reliably from a window than from the app
/// on Linux. Tests set the theme themselves, so they skip the first match.
pub fn observe_appearance<V: 'static>(window: &mut Window, cx: &mut Context<V>) -> Subscription {
    if !cfg!(test) {
        follow_system(Some(window), cx);
    }
    cx.observe_window_appearance(window, |_, window, cx| {
        follow_system(Some(window), cx);
    })
}

/// Text in Geist at a design size: `size` and `line` in pixels.
pub fn text(size: f32, line: f32, color: Hsla) -> Div {
    div()
        .font_family(SANS)
        .text_size(px(size))
        .line_height(px(line))
        .text_color(color)
}

/// Text in Geist Mono.
pub fn mono(size: f32, line: f32, color: Hsla) -> Div {
    text(size, line, color).font_family(MONO)
}

/// The height of a text field, matching [`select`].
pub const FIELD_HEIGHT: f32 = 28.;
/// The height of a small text field, such as Quick convert's file name.
pub const SMALL_FIELD_HEIGHT: f32 = 24.;

/// A text field. The component sizes its height in rems but its padding in
/// pixels, so at convt's 13px rem the default leaves less than a line of room
/// and cuts off descenders. A fixed height with no vertical padding fits the
/// line. (`Input::h` sets only a multi-line height, hence `Styled::h`.)
pub fn field(state: &Entity<InputState>, id: &'static str) -> Input {
    sized_field(Input::new(state).id(id), FIELD_HEIGHT)
}

/// A small text field; see [`field`].
pub fn small_field(state: &Entity<InputState>, id: &'static str) -> Input {
    sized_field(Input::new(state).id(id).small(), SMALL_FIELD_HEIGHT)
}

fn sized_field(input: Input, height: f32) -> Input {
    Styled::h(input, px(height)).py(px(0.))
}

/// A 1px ring drawn inside an element, like the design's inset box shadows.
pub fn inset_ring(color: Hsla, width: f32) -> BoxShadow {
    BoxShadow {
        color,
        offset: point(px(0.), px(0.)),
        blur_radius: px(0.),
        spread_radius: px(width),
        inset: true,
    }
}

fn shadow(color: Hsla, y: f32, blur: f32) -> BoxShadow {
    BoxShadow {
        color,
        offset: point(px(0.), px(y)),
        blur_radius: px(blur),
        spread_radius: px(0.),
        inset: false,
    }
}

/// An element registered for test queries: [`clickable`] and the controls
/// built on it.
pub type Clickable = gpui_kit::base::ObservedElement<Stateful<Div>>;

/// A clickable element that tests can find by `id` and read by `label`.
pub fn clickable(id: impl Into<ElementId>, label: impl Into<SharedString>) -> Clickable {
    let label = label.into();
    div()
        .id(id)
        .cursor_pointer()
        .test_support()
        .aria_label(label)
}

/// The green call-to-action button.
///
/// The white label on this green has about 3:1 contrast, below the 4.5:1 AA
/// target for text this size. Leo has not chosen the fix yet (a darker
/// gradient or a dark label), so this matches the design for now.
pub fn primary_button(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    size: f32,
    disabled: bool,
) -> Clickable {
    let label = label.into();
    clickable(id, label.clone())
        .px(px(14.))
        .py(px(6.))
        .rounded(px(8.))
        .border_1()
        .border_color(c(0x157F4A))
        .bg(linear_gradient(
            180.,
            linear_color_stop(c(0x2AB673), 0.),
            linear_color_stop(c(0x1A9A5B), 1.),
        ))
        .shadow(vec![
            BoxShadow {
                color: ca(0xFFFFFF47),
                offset: point(px(0.), px(1.)),
                blur_radius: px(0.),
                spread_radius: px(0.),
                inset: true,
            },
            shadow(ca(0x0A3C2340), 1., 2.),
        ])
        .when(disabled, |d| d.opacity(0.5).cursor_default())
        .child(
            text(size, 16., c(0xFFFFFF))
                .font_weight(FontWeight::SEMIBOLD)
                .whitespace_nowrap()
                .child(label),
        )
}

/// The white (or dark) bordered button, such as Cancel.
pub fn secondary_button(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    p: &Palette,
) -> Clickable {
    let label = label.into();
    clickable(id, label.clone())
        .px(px(14.))
        .py(px(6.))
        .rounded(px(7.))
        .bg(p.control)
        .border_1()
        .border_color(p.control_border)
        .shadow(vec![shadow(
            if p.dark {
                ca(0x00000066)
            } else {
                ca(0x0000000D)
            },
            1.,
            if p.dark { 2. } else { 1. },
        )])
        .child(text(13., 16., p.text).whitespace_nowrap().child(label))
}

/// A text-only button, such as "Clear finished", "Back" or "Change".
pub fn text_button(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    color: Hsla,
    size: f32,
) -> Clickable {
    let label = label.into();
    clickable(id, label.clone()).child(text(size, 16., color).whitespace_nowrap().child(label))
}

/// A small switch (`small`: 28x16 as in lists, otherwise 30x18).
pub fn switch(id: impl Into<ElementId>, on: bool, small: bool, p: &Palette) -> Clickable {
    let (w, h, knob) = if small {
        (28., 16., 12.)
    } else {
        (30., 18., 14.)
    };
    div()
        .id(id)
        .cursor_pointer()
        .test_support()
        .aria_toggled(if on {
            accesskit::Toggled::True
        } else {
            accesskit::Toggled::False
        })
        .aria_label(if on { "On" } else { "Off" })
        .flex()
        .flex_shrink_0()
        .items_center()
        .w(px(w))
        .h(px(h))
        .p(px(2.))
        .rounded(px(h / 2.))
        .bg(if on { p.control_on } else { p.toggle_off })
        .when(on, |d| d.justify_end())
        .child(
            div()
                .size(px(knob))
                .rounded(px(knob / 2.))
                .bg(c(0xFFFFFF))
                .shadow(vec![shadow(ca(0x00000033), 1., 2.)]),
        )
}

/// A 14px checkbox with its label.
pub fn checkbox(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    checked: bool,
    p: &Palette,
) -> Clickable {
    let label = label.into();
    let mark = if checked {
        div()
            .flex()
            .items_center()
            .justify_center()
            .size(px(14.))
            .rounded(px(4.))
            .bg(p.control_on)
            .child(
                Icon::new(IconName::Check)
                    .size(px(10.))
                    .text_color(c(0xFFFFFF)),
            )
    } else {
        div()
            .size(px(14.))
            .rounded(px(4.))
            .bg(p.control)
            .shadow(vec![inset_ring(p.mark_off, 1.)])
    };
    div()
        .id(id)
        .cursor_pointer()
        .test_support()
        .aria_label(label.clone())
        .aria_toggled(if checked {
            accesskit::Toggled::True
        } else {
            accesskit::Toggled::False
        })
        .flex()
        .items_center()
        .gap(px(8.))
        .child(mark.flex_shrink_0())
        .child(text(13., 16., p.text).child(label))
}

/// A 4px progress bar. `fraction` is 0 to 1.
pub fn progress(fraction: f32, track: Hsla, fill: Hsla) -> Div {
    let fraction = fraction.clamp(0., 1.);
    div()
        .flex()
        .flex_shrink_0()
        .h(px(4.))
        .rounded(px(2.))
        .bg(track)
        .child(
            div()
                .h(px(4.))
                .rounded(px(2.))
                .bg(fill)
                .w(relative(fraction)),
        )
}

/// One option of a [`select`].
pub struct Choice {
    pub id: SharedString,
    pub label: SharedString,
}

impl Choice {
    pub fn new(id: impl Into<SharedString>, label: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
        }
    }
}

/// A dropdown box showing `value`. Clicking it calls `on_toggle`; while
/// `open`, it lists `choices` below, each with the id `{id}-{choice id}`.
#[allow(clippy::too_many_arguments)]
pub fn select(
    id: &str,
    value: impl Into<SharedString>,
    width: f32,
    mono_value: bool,
    open: bool,
    choices: Vec<Choice>,
    p: &Palette,
    on_toggle: impl Fn(&mut Window, &mut App) + 'static,
    on_pick: impl Fn(&str, &mut Window, &mut App) + Clone + 'static,
) -> Div {
    let value = value.into();
    let label = if mono_value {
        mono(12., 16., p.text)
    } else {
        text(12., 16., p.text)
    };
    let button = clickable(SharedString::from(id.to_string()), value.clone())
        .flex()
        .flex_shrink_0()
        .items_center()
        .justify_between()
        .w(px(width))
        .h(px(FIELD_HEIGHT))
        .px(px(10.))
        .rounded(px(6.))
        .bg(p.control)
        .border_1()
        .border_color(p.control_border)
        .on_click(move |_, window, cx| on_toggle(window, cx))
        // A folder keeps its last, most telling part.
        .child(
            label
                .overflow_hidden()
                .whitespace_nowrap()
                .text_ellipsis_start()
                .child(value),
        )
        .child(
            Icon::new(IconName::ChevronDown)
                .size(px(10.))
                .text_color(p.secondary),
        );
    let menu = open.then(|| {
        let items = choices.into_iter().map(|choice| {
            let on_pick = on_pick.clone();
            let pick = choice.id.clone();
            clickable(
                SharedString::from(format!("{id}-{}", choice.id)),
                choice.label.clone(),
            )
            .px(px(10.))
            .py(px(4.))
            .rounded(px(4.))
            .hover(|s| s.bg(gpui_kit::transparent_black().opacity(0.06)))
            .on_click(move |_, window, cx| on_pick(&pick, window, cx))
            .child(if mono_value {
                mono(12., 16., p.text).child(choice.label)
            } else {
                text(12., 16., p.text).child(choice.label)
            })
        });
        // An anchored element sits at the top of its container, over the
        // box, unless it is moved down by the box's height.
        deferred(
            anchored()
                .offset(point(px(0.), px(FIELD_HEIGHT)))
                .snap_to_window()
                .child(
                    div()
                        .id(SharedString::from(format!("{id}-menu")))
                        .occlude()
                        .mt(px(4.))
                        .w(px(width))
                        .p(px(4.))
                        .rounded(px(8.))
                        .bg(p.control)
                        .border_1()
                        .border_color(p.control_border)
                        .shadow(vec![shadow(p.shadow_soft, 8., 24.)])
                        .children(items),
                ),
        )
        .with_priority(1)
    });
    div().flex().flex_col().child(button).children(menu)
}

/// A segmented control: one button per choice, the selected one raised.
pub fn segmented(
    id: &str,
    choices: &[(&'static str, SharedString)],
    selected: &str,
    p: &Palette,
    on_pick: impl Fn(&'static str, &mut Window, &mut App) + Clone + 'static,
) -> Div {
    div()
        .flex()
        .p(px(2.))
        .rounded(px(7.))
        .bg(p.segmented)
        .when(p.dark, |d| {
            d.shadow(vec![inset_ring(p.recessed_border, 1.)])
        })
        .children(choices.iter().map(|(key, label)| {
            let (key, label) = (*key, label.clone());
            let on = key == selected;
            let on_pick = on_pick.clone();
            clickable(SharedString::from(format!("{id}-{key}")), label.clone())
                .aria_selected(on)
                .px(px(14.))
                .py(px(5.))
                .rounded(px(5.))
                .when(on, |d| {
                    d.bg(p.segmented_selected).shadow(vec![
                        shadow(p.shadow_soft, 0., 0.5),
                        shadow(ca(0x00000014), 1., 2.),
                    ])
                })
                .on_click(move |_, window, cx| on_pick(key, window, cx))
                .child(
                    text(12., 16., if on { p.text } else { p.secondary })
                        .when(on, |d| d.font_weight(FontWeight::MEDIUM))
                        .child(label),
                )
        }))
}

/// A thumbnail for a file: the image itself for pictures, a frame for video
/// on a local disk (see `thumbs.rs`), and a colored badge with the extension
/// for everything else, including pictures that can't be decoded and videos
/// whose frame isn't ready.
pub fn thumbnail(path: &std::path::Path, w: f32, h: f32, p: &Palette) -> AnyElement {
    use convt_core::Category;
    let format = convt_core::format_by_extension(path);
    let category = format.map(|f| f.category);
    match category {
        Some(Category::Image) if path.exists() => {
            let (path, p) = (path.to_path_buf(), *p);
            img(path.clone())
                .w(px(w))
                .h(px(h))
                .flex_shrink_0()
                .rounded(px(5.))
                .object_fit(ObjectFit::Cover)
                .with_fallback(move || badge_tile(&path, category, w, h, &p))
                .into_any_element()
        }
        Some(Category::Video) => match crate::thumbs::video_frame(path) {
            Some(frame) => {
                let (path, p) = (path.to_path_buf(), *p);
                div()
                    .flex_shrink_0()
                    .w(px(w))
                    .h(px(h))
                    .rounded(px(5.))
                    .overflow_hidden()
                    .bg(c(0x141414))
                    .when(p.dark, |d| d.shadow(vec![inset_ring(ca(0xFFFFFF14), 1.)]))
                    .child(
                        img(frame)
                            .w(px(w))
                            .h(px(h))
                            .rounded(px(5.))
                            .object_fit(ObjectFit::Cover)
                            .with_fallback(move || badge_tile(&path, category, w, h, &p)),
                    )
                    .into_any_element()
            }
            None => badge_tile(path, category, w, h, p),
        },
        _ => badge_tile(path, category, w, h, p),
    }
}

fn badge_tile(
    path: &std::path::Path,
    category: Option<convt_core::Category>,
    w: f32,
    h: f32,
    p: &Palette,
) -> AnyElement {
    use convt_core::Category;
    let ext = path
        .extension()
        .map(|e| e.to_string_lossy().to_uppercase())
        .unwrap_or_default();
    let badge = match category {
        Some(Category::Audio) => c(0x7A4BC2),
        Some(Category::Video) => c(0x3A3F3C),
        Some(Category::Pdf) => c(0xC9372C),
        Some(Category::Spreadsheet) => c(0x2E7D4F),
        Some(Category::Presentation) => c(0xD0702C),
        _ => c(0x2B5BB8),
    };
    div()
        .flex()
        .items_center()
        .justify_center()
        .w(px(w))
        .h(px(h))
        .flex_shrink_0()
        .rounded(px(5.))
        .bg(p.thumb)
        .shadow(vec![inset_ring(p.thumb_border, 1.)])
        .child(
            div().px(px(3.)).py(px(1.)).rounded(px(2.)).bg(badge).child(
                mono(8., 10., c(0xFFFFFF))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(ext.chars().take(4).collect::<String>()),
            ),
        )
        .into_any_element()
}

/// A title bar drawn in the window, for macOS, where windows use a
/// transparent title bar to match the design. Other platforms keep their
/// native title bar, so this draws nothing there.
pub fn title_bar(title: Option<&str>, height: f32, bg: Option<Hsla>, p: &Palette) -> Option<Div> {
    if !cfg!(target_os = "macos") {
        return None;
    }
    Some(
        div()
            .flex()
            .flex_shrink_0()
            .items_center()
            .justify_center()
            .h(px(height))
            .when_some(bg, |d, bg| d.bg(bg))
            .children(title.map(|t| {
                text(13., 16., p.text)
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(t.to_string())
            })),
    )
}

/// Whether windows draw their own title bar (see [`title_bar`]).
pub fn transparent_titlebar() -> bool {
    cfg!(target_os = "macos")
}

/// "this Mac" or "this computer", for copy that names the machine.
pub fn this_machine() -> &'static str {
    if cfg!(target_os = "macos") {
        "this Mac"
    } else {
        "this computer"
    }
}

/// "Finder" or "your file manager".
pub fn file_manager() -> &'static str {
    if cfg!(target_os = "macos") {
        "Finder"
    } else {
        "your file manager"
    }
}

/// "Finder" or "the file manager", for "Reveal it in …".
pub fn file_manager_name() -> &'static str {
    if cfg!(target_os = "macos") {
        "Finder"
    } else {
        "the file manager"
    }
}
