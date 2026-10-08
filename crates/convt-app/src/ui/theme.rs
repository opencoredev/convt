//! The convt look: one visual system for every window. It holds the light and
//! dark palettes, the bundled Inter and Geist Mono fonts, the Hugeicons set,
//! the type and spacing scale, and the controls the windows share. The theme
//! follows the system appearance.
//!
//! The system, in short: neutral surfaces with one accent (the brand green),
//! Inter for words and Geist Mono for file facts (sizes, formats, paths).
//! Sizes come from [`space`] and [`radius`]; text from [`text`] and [`mono`]
//! at the sizes in [`size`]. Lists and settings sit in [`group`]s of
//! [`row`]s; anything that needs the user's attention is a [`callout`].

use std::borrow::Cow;

use gpui_kit::component::input::{Input, InputState};
use gpui_kit::component::spinner::Spinner;
use gpui_kit::component::{ActiveTheme, Icon, IconNamed, Sizable, Theme};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

pub const SANS: &str = "Inter";
pub const MONO: &str = "Geist Mono";

/// Inter 4.1 and Geist Mono 1.7.2, under the SIL Open Font License 1.1
/// (`assets/fonts/OFL.txt`).
const FONTS: [&[u8]; 7] = [
    include_bytes!("../../assets/fonts/Inter-Regular.ttf"),
    include_bytes!("../../assets/fonts/Inter-Medium.ttf"),
    include_bytes!("../../assets/fonts/Inter-SemiBold.ttf"),
    include_bytes!("../../assets/fonts/Inter-Bold.ttf"),
    include_bytes!("../../assets/fonts/GeistMono-Regular.ttf"),
    include_bytes!("../../assets/fonts/GeistMono-Medium.ttf"),
    include_bytes!("../../assets/fonts/GeistMono-SemiBold.ttf"),
];

/// The spacing scale, in pixels.
pub mod space {
    pub const XS: f32 = 4.;
    pub const SM: f32 = 8.;
    pub const MD: f32 = 12.;
    pub const LG: f32 = 16.;
    pub const XL: f32 = 24.;
    pub const XXL: f32 = 32.;
}

/// Corner radii, in pixels.
pub mod radius {
    pub const SM: f32 = 5.;
    pub const CONTROL: f32 = 7.;
    pub const CARD: f32 = 10.;
    pub const PANEL: f32 = 14.;
}

/// Type sizes and their line heights, in pixels.
pub mod size {
    /// Window and page titles.
    pub const TITLE: (f32, f32) = (17., 22.);
    /// First-run headlines.
    pub const DISPLAY: (f32, f32) = (22., 28.);
    /// Row titles and body copy.
    pub const BODY: (f32, f32) = (13., 18.);
    /// Descriptions, buttons and secondary copy.
    pub const SMALL: (f32, f32) = (12., 17.);
    /// Metadata, badges and section labels.
    pub const CAPTION: (f32, f32) = (11., 14.);
}

/// Every color the windows use, for one appearance.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Palette {
    pub dark: bool,
    /// The content area of every window.
    pub window: Hsla,
    /// The sidebar, the Settings toolbar and drawn title bars.
    pub chrome: Hsla,
    pub chrome_border: Hsla,
    /// Footers, strips and wells set into a surface.
    pub recessed: Hsla,
    /// Cards, groups and controls.
    pub surface: Hsla,
    /// Card and group outlines.
    pub border: Hsla,
    /// Control outlines, a step stronger than `border`.
    pub control_border: Hsla,
    /// Section dividers.
    pub hairline: Hsla,
    /// Dividers between rows of a group or list.
    pub row_divider: Hsla,
    /// Neutral badges and tags, and their outline.
    pub chip: Hsla,
    pub chip_border: Hsla,
    pub text: Hsla,
    pub secondary: Hsla,
    pub tertiary: Hsla,
    /// Hovered rows and ghost buttons.
    pub hover: Hsla,
    /// The selected nav item or tab.
    pub selected: Hsla,
    /// The accent as a fill: progress, switches, icons.
    pub green: Hsla,
    /// The accent as text and links; at least 4.5:1 on `window`.
    pub green_text: Hsla,
    pub green_tint: Hsla,
    pub green_border: Hsla,
    pub error: Hsla,
    pub error_tint: Hsla,
    pub error_border: Hsla,
    pub track: Hsla,
    pub toggle_off: Hsla,
    pub mark_off: Hsla,
    pub thumb: Hsla,
    pub thumb_border: Hsla,
    /// Menus and the menu bar popover.
    pub overlay: Hsla,
    pub shadow: Hsla,
    pub shadow_soft: Hsla,
}

fn c(hex: u32) -> Hsla {
    rgb(hex).into()
}

fn ca(hex: u32) -> Hsla {
    rgba(hex).into()
}

/// Both palettes are the convt.app tokens (`apps/web/src/styles.css`); the
/// comments name the web token each color comes from. The few colors the web
/// has no token for are marked "derived".
impl Palette {
    pub fn light() -> Self {
        Self {
            dark: false,
            window: c(0xFFFFFF),         // --page
            chrome: c(0xF7F8F7),         // --sunken
            chrome_border: c(0xE6E8E7),  // --line
            recessed: c(0xF7F8F7),       // --sunken
            surface: c(0xFFFFFF),        // --raised
            border: c(0xE6E8E7),         // --line
            control_border: c(0xD5D9D7), // --line-strong
            hairline: c(0xE6E8E7),       // --line
            row_divider: c(0xEEF0EF),    // --divider
            chip: c(0xF5F7F6),           // --chip
            chip_border: c(0xE0E3E1),    // --chip-line
            text: c(0x0A0A0A),           // --ink
            secondary: c(0x6B6F6D),      // --ink-2
            tertiary: c(0x6C716E),       // --ink-3
            hover: c(0xF3F4F3),          // --hover
            selected: c(0xE6E8E7),       // --line
            green: c(0x127A47),          // --green
            green_text: c(0x127A47),     // --green
            green_tint: c(0xEEF7F2),     // --green-tint
            green_border: c(0xCFE6D9),   // --green-line
            error: c(0xB3261E),          // --error
            error_tint: c(0xFCF3F2),     // derived: --error over --page, as --green-tint
            error_border: c(0xF0D4D1),   // --error-line
            track: c(0xE6E8E7),          // --line
            toggle_off: c(0xD0D3D1),     // --separator
            // Derived: a step past --line-strong, so an empty box keeps its edge.
            mark_off: c(0xB4B9B6),
            thumb: c(0xFFFFFF),          // --raised
            thumb_border: c(0xE0E3E1),   // --chip-line
            overlay: c(0xFFFFFF),        // --raised
            shadow: ca(0x0A3C231F),      // --shadow-float
            shadow_soft: ca(0x0A1E1426), // --shadow-note
        }
    }

    pub fn dark() -> Self {
        Self {
            dark: true,
            window: c(0x0A0B0B),          // --page
            chrome: c(0x111312),          // --raised
            chrome_border: c(0x232726),   // --line
            recessed: c(0x161918),        // --sunken
            surface: c(0x111312),         // --raised
            border: c(0x232726),          // --line
            control_border: c(0x2E3331),  // --line-strong
            hairline: c(0x232726),        // --line
            row_divider: c(0x232726),     // --divider
            chip: c(0x161918),            // --chip
            chip_border: c(0x2E3331),     // --chip-line
            text: c(0xEDEFEE),            // --ink
            secondary: c(0xA1A6A3),       // --ink-2
            tertiary: c(0x868B88),        // --ink-3
            hover: c(0x1C201E),           // --hover
            selected: c(0x232726),        // --line
            green: c(0x3FCB84),           // --green
            green_text: c(0x3FCB84),      // --green
            green_tint: c(0x12261B),      // --green-tint
            green_border: ca(0x3FCB8433), // --green-line
            error: c(0xF2786D),           // --error
            error_tint: c(0x261716),      // derived: --error over --page, as --green-tint
            error_border: ca(0xF2786D33), // --error-line
            track: c(0x232726),           // --line
            toggle_off: c(0x2E3331),      // --separator
            // Derived: a step past --line-strong, so an empty box keeps its edge.
            mark_off: c(0x5A605D),
            thumb: c(0x161918),           // --sunken
            thumb_border: ca(0xFFFFFF14), // --shadow-float ring
            overlay: c(0x1C201E),         // --hover, as the web's floating cards
            shadow: ca(0x00000099),       // --shadow-float
            shadow_soft: ca(0x00000066),  // --shadow-float
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
        tracing::warn!(error = %e, "could not load the bundled fonts");
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
        t.radius = px(radius::CONTROL);
        t.background = p.surface;
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

/// Text in Inter at a design size: `size` and `line` in pixels.
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

/// Text at one of the [`size`] steps.
pub fn styled(step: (f32, f32), color: Hsla) -> Div {
    text(step.0, step.1, color)
}

/// The icons the windows draw: Hugeicons' free stroke-rounded set (MIT),
/// written to `assets/icons/hugeicons` by `assets/icons/generate.mjs`. Every
/// window draws these, never gpui-kit's Lucide set; `ui::assets` also serves
/// them at the Lucide paths the component library's own controls load.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IconName {
    ArrowDown,
    ArrowRight,
    Ban,
    Bot,
    Calendar,
    Check,
    ChevronDown,
    ChevronRight,
    ChevronsUpDown,
    CircleAlert,
    CircleCheck,
    CircleUser,
    CircleX,
    Close,
    Cloud,
    Computer,
    Document,
    Download,
    Edit,
    ExternalLink,
    Folder,
    FolderOpen,
    Google,
    HardDrive,
    Inbox,
    Info,
    Key,
    Loader,
    Mail,
    Minus,
    Plus,
    RefreshCw,
    RotateCw,
    Settings,
    Sparkles,
    Star,
    TriangleAlert,
    WindowMaximize,
    WindowRestore,
}

impl IconName {
    pub const ALL: [IconName; 39] = {
        use IconName::*;
        [
            ArrowDown,
            ArrowRight,
            Ban,
            Bot,
            Calendar,
            Check,
            ChevronDown,
            ChevronRight,
            ChevronsUpDown,
            CircleAlert,
            CircleCheck,
            CircleUser,
            CircleX,
            Close,
            Cloud,
            Computer,
            Document,
            Download,
            Edit,
            ExternalLink,
            Folder,
            FolderOpen,
            Google,
            HardDrive,
            Inbox,
            Info,
            Key,
            Loader,
            Mail,
            Minus,
            Plus,
            RefreshCw,
            RotateCw,
            Settings,
            Sparkles,
            Star,
            TriangleAlert,
            WindowMaximize,
            WindowRestore,
        ]
    };

    /// The file name in `assets/icons/hugeicons`, without `.svg`.
    pub fn file(self) -> &'static str {
        use IconName::*;
        match self {
            ArrowDown => "arrow-down",
            ArrowRight => "arrow-right",
            Ban => "cancel-circle",
            Bot => "magic-wand",
            Calendar => "calendar",
            Check => "check",
            ChevronDown => "chevron-down",
            ChevronRight => "chevron-right",
            ChevronsUpDown => "chevrons-up-down",
            CircleAlert => "alert-circle",
            CircleCheck => "check-circle",
            CircleUser => "user-circle",
            CircleX => "cancel-circle",
            Close => "cancel",
            Cloud => "cloud",
            Computer => "computer",
            Document => "document",
            Download => "download",
            Edit => "edit",
            ExternalLink => "external-link",
            Folder => "folder",
            FolderOpen => "folder-open",
            Google => "google",
            HardDrive => "hard-drive",
            Inbox => "inbox",
            Info => "info",
            Key => "key",
            Loader => "loading",
            Mail => "mail",
            Minus => "minus",
            Plus => "add",
            RefreshCw => "refresh",
            RotateCw => "rotate",
            Settings => "settings",
            Sparkles => "sparkles",
            Star => "star",
            TriangleAlert => "alert-triangle",
            WindowMaximize => "square",
            WindowRestore => "restore",
        }
    }
}

impl IconNamed for IconName {
    fn path(self) -> SharedString {
        format!("icons/hugeicons/{}.svg", self.file()).into()
    }
}

/// An icon from the bundled Hugeicons set (see [`IconName`]).
pub fn icon(name: IconName, size: f32, color: Hsla) -> Icon {
    Icon::new(name).size(px(size)).text_color(color)
}

/// Google's four-color G, for "Continue with Google". Drawn as an image,
/// because icons take one color.
pub fn google_mark(size: f32) -> Img {
    img("icons/google-g.svg").size(px(size)).flex_shrink_0()
}

/// The height of a text field, matching [`select`].
pub const FIELD_HEIGHT: f32 = 30.;
/// The height of a small text field, such as Quick convert's file name.
pub const SMALL_FIELD_HEIGHT: f32 = 26.;

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

/// A ring drawn inside an element, like the design's inset box shadows.
pub fn inset_ring(color: Hsla, width: f32) -> BoxShadow {
    BoxShadow {
        color,
        offset: point(px(0.), px(0.)),
        blur_radius: px(0.),
        spread_radius: px(width),
        inset: true,
    }
}

pub fn shadow(color: Hsla, y: f32, blur: f32) -> BoxShadow {
    BoxShadow {
        color,
        offset: point(px(0.), px(y)),
        blur_radius: px(blur),
        spread_radius: px(0.),
        inset: false,
    }
}

/// The resting shadow of raised controls and cards (the web's
/// `--shadow-button` drop).
fn raise(p: &Palette) -> Vec<BoxShadow> {
    vec![shadow(
        if p.dark {
            ca(0x00000066)
        } else {
            ca(0x0000000F)
        },
        1.,
        2.,
    )]
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

/// The top and bottom of the primary button's gradient.
const PRIMARY_FILL: (u32, u32) = (0x127A47, 0x0F6B3E);

/// How a [`Button`] looks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Look {
    /// The one main action of a window or card, in the brand green.
    Primary,
    /// A bordered button on the surface, such as Cancel.
    Secondary,
    /// Text that gains a background on hover, for row actions.
    Ghost,
}

/// A button: a look, a label, an optional leading icon and two heights.
pub struct Button {
    id: ElementId,
    label: SharedString,
    look: Look,
    icon: Option<IconName>,
    small: bool,
    disabled: bool,
    loading: bool,
    color: Option<Hsla>,
}

impl Button {
    pub fn new(id: impl Into<ElementId>, label: impl Into<SharedString>, look: Look) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            look,
            icon: None,
            small: false,
            disabled: false,
            loading: false,
            color: None,
        }
    }

    pub fn primary(id: impl Into<ElementId>, label: impl Into<SharedString>) -> Self {
        Self::new(id, label, Look::Primary)
    }

    pub fn secondary(id: impl Into<ElementId>, label: impl Into<SharedString>) -> Self {
        Self::new(id, label, Look::Secondary)
    }

    pub fn ghost(id: impl Into<ElementId>, label: impl Into<SharedString>) -> Self {
        Self::new(id, label, Look::Ghost)
    }

    pub fn icon(mut self, icon: IconName) -> Self {
        self.icon = Some(icon);
        self
    }

    /// 26px tall instead of 30.
    pub fn small(mut self) -> Self {
        self.small = true;
        self
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    /// Busy: a spinner in place of the icon, and no hover. Give it a label
    /// that says what it's doing, such as "Checking…".
    pub fn loading(mut self, loading: bool) -> Self {
        self.loading = loading;
        self
    }

    /// The label color of a ghost or secondary button, such as red for Remove.
    pub fn color(mut self, color: Hsla) -> Self {
        self.color = Some(color);
        self
    }

    pub fn build(self, p: &Palette) -> Clickable {
        let (h, pad, size) = if self.small {
            (26., 10., 12.)
        } else {
            (30., 14., 13.)
        };
        let fg = match self.look {
            Look::Primary => c(0xFFFFFF),
            Look::Secondary => self.color.unwrap_or(p.text),
            Look::Ghost => self.color.unwrap_or(p.secondary),
        };
        let (hover_fg, ghost) = (self.color.unwrap_or(p.text), self.look == Look::Ghost);
        let still = self.disabled || self.loading;
        let base = clickable(self.id, self.label.clone())
            .flex()
            .flex_shrink_0()
            .items_center()
            .justify_center()
            .gap(px(6.))
            .h(px(h))
            .px(px(if ghost { pad - 2. } else { pad }))
            .rounded(px(radius::CONTROL));
        let base = match self.look {
            // The web's `.btn-primary` ring and shadows. Its fill (#22A867 to
            // #1B9259) gives white text about 3:1, so the fill starts at
            // --green instead: at least 5.3:1, past the AA target of 4.5.
            Look::Primary => base
                .bg(linear_gradient(
                    180.,
                    linear_color_stop(c(PRIMARY_FILL.0), 0.),
                    linear_color_stop(c(PRIMARY_FILL.1), 1.),
                ))
                .shadow(vec![
                    BoxShadow {
                        color: ca(0xFFFFFF47),
                        offset: point(px(0.), px(1.)),
                        blur_radius: px(0.),
                        spread_radius: px(0.),
                        inset: true,
                    },
                    inset_ring(c(0x157F4A), 1.),
                    shadow(ca(0x0A3C2340), 1., 2.),
                    shadow(ca(0x0A3C231F), 2., 6.),
                ])
                .when(!still, |d| d.hover(|s| s.opacity(0.92))),
            Look::Secondary => base
                .bg(p.surface)
                .shadow({
                    let mut s = raise(p);
                    s.push(inset_ring(p.control_border, 1.));
                    s
                })
                .when(!still, |d| d.hover(|s| s.bg(p.recessed))),
            Look::Ghost => base.when(!still, |d| d.hover(|s| s.bg(p.hover))),
        };
        let icon_color = if self.look == Look::Secondary {
            p.secondary
        } else {
            fg
        };
        let icon_size = if self.small { 13. } else { 14. };
        base.when(self.disabled, |d| d.opacity(0.45))
            .when(still, |d| d.cursor_default())
            .when(self.loading, |d| {
                d.child(Spinner::new().with_size(px(icon_size)).color(icon_color))
            })
            .when(!self.loading, |d| {
                d.children(self.icon.map(|i| icon(i, icon_size, icon_color)))
            })
            .child(
                text(size, 16., fg)
                    .font_weight(if self.look == Look::Primary {
                        FontWeight::SEMIBOLD
                    } else {
                        FontWeight::MEDIUM
                    })
                    .whitespace_nowrap()
                    .when(ghost && !still, |d| d.hover(|s| s.text_color(hover_fg)))
                    .child(self.label),
            )
    }
}

/// The green call-to-action button, 30px tall.
pub fn primary_button(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    p: &Palette,
) -> Clickable {
    Button::primary(id, label).build(p)
}

/// The bordered button, such as Cancel.
pub fn secondary_button(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    p: &Palette,
) -> Clickable {
    Button::secondary(id, label).build(p)
}

/// A text-only link button, such as "Change" or "Renew".
pub fn text_button(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    color: Hsla,
    size: f32,
) -> Clickable {
    let label = label.into();
    clickable(id, label.clone()).child(
        text(size, 16., color)
            .font_weight(FontWeight::MEDIUM)
            .whitespace_nowrap()
            .hover(|s| s.opacity(0.75))
            .child(label),
    )
}

/// A switch (`small`: 28x16, as in lists, otherwise 32x18). Screen readers
/// read `name`; the state is in `aria_toggled`.
pub fn switch(
    id: impl Into<ElementId>,
    name: impl Into<SharedString>,
    on: bool,
    small: bool,
    p: &Palette,
) -> Clickable {
    let (w, h, knob) = if small {
        (28., 16., 12.)
    } else {
        (32., 18., 14.)
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
        .aria_label(name)
        .flex()
        .flex_shrink_0()
        .items_center()
        .w(px(w))
        .h(px(h))
        .p(px(2.))
        .rounded(px(h / 2.))
        .bg(if on { p.green } else { p.toggle_off })
        .when(on, |d| d.justify_end())
        .child(
            div()
                .size(px(knob))
                .rounded(px(knob / 2.))
                .bg(c(0xFFFFFF))
                .shadow(vec![shadow(ca(0x00000040), 1., 2.)]),
        )
}

/// A 15px checkbox with its label.
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
            .size(px(15.))
            .rounded(px(4.))
            .bg(p.green)
            .child(icon(IconName::Check, 11., c(0xFFFFFF)))
    } else {
        div()
            .size(px(15.))
            .rounded(px(4.))
            .bg(p.surface)
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
        .overflow_hidden()
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
    let current = value.clone();
    let button = clickable(SharedString::from(id.to_string()), value.clone())
        .flex()
        .flex_shrink_0()
        .items_center()
        .justify_between()
        .gap(px(8.))
        .w(px(width))
        .h(px(FIELD_HEIGHT))
        .px(px(10.))
        .rounded(px(radius::CONTROL))
        .bg(p.surface)
        .shadow({
            let mut s = raise(p);
            s.push(inset_ring(
                if open { p.green } else { p.control_border },
                1.,
            ));
            s
        })
        .hover(|s| s.bg(p.recessed))
        .on_click(move |_, window, cx| on_toggle(window, cx))
        // A folder keeps its last, most telling part.
        .child(
            label
                .overflow_hidden()
                .whitespace_nowrap()
                .text_ellipsis_start()
                .child(value),
        )
        .child(icon(IconName::ChevronsUpDown, 12., p.tertiary).flex_shrink_0());
    let menu = open.then(|| {
        let items = choices.into_iter().map(|choice| {
            let on_pick = on_pick.clone();
            let pick = choice.id.clone();
            let on = choice.label == current;
            clickable(
                SharedString::from(format!("{id}-{}", choice.id)),
                choice.label.clone(),
            )
            .flex()
            .items_center()
            .justify_between()
            .gap(px(8.))
            .px(px(8.))
            .h(px(28.))
            .rounded(px(radius::SM))
            .hover(|s| s.bg(p.hover))
            .on_click(move |_, window, cx| on_pick(&pick, window, cx))
            .child(if mono_value {
                mono(12., 16., p.text).child(choice.label)
            } else {
                text(12., 16., p.text).child(choice.label)
            })
            .children(on.then(|| icon(IconName::Check, 12., p.green_text)))
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
                        .rounded(px(radius::CARD))
                        .bg(p.overlay)
                        .shadow(vec![
                            inset_ring(p.border, 1.),
                            shadow(p.shadow_soft, 8., 24.),
                        ])
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
        .flex_shrink_0()
        .p(px(2.))
        .gap(px(2.))
        .rounded(px(radius::CONTROL + 1.))
        .bg(if p.dark { p.recessed } else { p.track })
        .when(p.dark, |d| d.shadow(vec![inset_ring(p.border, 1.)]))
        .children(choices.iter().map(|(key, label)| {
            let key = *key;
            let on = key == selected;
            let on_pick = on_pick.clone();
            clickable(SharedString::from(format!("{id}-{key}")), label.clone())
                .aria_selected(on)
                .flex()
                .items_center()
                .h(px(24.))
                .px(px(12.))
                .rounded(px(radius::SM + 1.))
                .when(on, |d| {
                    d.bg(if p.dark { p.selected } else { p.surface })
                        .shadow(raise(p))
                })
                .when(!on, |d| d.hover(|s| s.bg(p.hover)))
                .on_click(move |_, window, cx| on_pick(key, window, cx))
                .child(
                    text(12., 16., if on { p.text } else { p.secondary })
                        .font_weight(FontWeight::MEDIUM)
                        .whitespace_nowrap()
                        .child(label.clone()),
                )
        }))
}

/// Keeps a hover flag in step with the pointer.
fn hover_setter(state: Entity<bool>) -> impl Fn(&bool, &mut Window, &mut App) + 'static {
    move |hovered, _, cx| {
        state.update(cx, |s, cx| {
            if *s != *hovered {
                *s = *hovered;
                cx.notify();
            }
        })
    }
}

/// Why a choice can't be picked, drawn over the window's other content with
/// its bottom left corner 8px above the choice's top left, so it never covers
/// the label it explains. Tests read it by `{element_id}-reason`.
fn reason_note(element_id: &SharedString, reason: SharedString, p: &Palette) -> AnyElement {
    div()
        .absolute()
        .top_0()
        .left_0()
        .child(deferred(
            anchored()
                .anchor(Anchor::BottomLeft)
                .snap_to_window_with_margin(px(8.))
                // Padding, not margin: `anchored` sizes to its child's box
                // and ignores margins.
                .child(
                    div().pb(px(8.)).child(
                        div()
                            .id(SharedString::from(format!("{element_id}-reason")))
                            .test_support()
                            .aria_label(reason.clone())
                            .px(px(8.))
                            .py(px(5.))
                            .rounded(px(radius::SM + 1.))
                            .bg(p.overlay)
                            .shadow(vec![
                                inset_ring(p.border, 1.),
                                shadow(p.shadow_soft, 4., 12.),
                            ])
                            .child(
                                styled(size::SMALL, p.text)
                                    .whitespace_nowrap()
                                    .child(reason),
                            ),
                    ),
                ),
        ))
        .into_any_element()
}

/// One choice of [`tiles`].
pub struct Tile {
    pub key: &'static str,
    pub icon: IconName,
    pub title: SharedString,
    pub line: SharedString,
    /// Why the choice can't be picked now; shown above it on hover.
    pub disabled: Option<SharedString>,
}

/// Side-by-side choices drawn as cards, each with a large icon, a title and
/// one line, such as where a conversion runs. The picked one has a green
/// ring. A disabled one is dimmed, ignores clicks and says why in a note
/// above it while hovered, so the note never covers the label it explains
/// (GPUI's own tooltip follows the pointer and, near a window's bottom edge,
/// lands on the control). Ids are `{id}-{key}`.
pub fn tiles(
    id: &str,
    choices: &[Tile],
    selected: &str,
    p: &Palette,
    window: &mut Window,
    cx: &mut App,
    on_pick: impl Fn(&'static str, &mut Window, &mut App) + Clone + 'static,
) -> Div {
    div()
        .flex()
        .gap(px(space::MD))
        .children(choices.iter().map(|tile| {
            let key = tile.key;
            let on = key == selected;
            let element_id = SharedString::from(format!("{id}-{key}"));
            let hover = tile.disabled.as_ref().map(|_| {
                let state = window.use_keyed_state(
                    SharedString::from(format!("{element_id}-hover")),
                    cx,
                    |_, _| false,
                );
                let shown = *state.read(cx);
                (state, shown)
            });
            let off = tile.disabled.is_some();
            let on_pick = on_pick.clone();
            let (tile_bg, tile_ring, glyph) = if on {
                (p.green_tint, p.green_border, p.green_text)
            } else {
                (p.chip, p.chip_border, p.secondary)
            };
            clickable(element_id.clone(), tile.title.clone())
                .aria_selected(on)
                .flex()
                .flex_1()
                .items_center()
                .gap(px(space::MD))
                .p(px(space::MD))
                .rounded(px(radius::CARD))
                .bg(p.surface)
                .shadow(if on {
                    vec![inset_ring(p.green, 1.5)]
                } else {
                    let mut s = raise(p);
                    s.push(inset_ring(p.border, 1.));
                    s
                })
                .when(!on && !off, |d| d.hover(|s| s.bg(p.recessed)))
                .when(off, |d| d.cursor_default())
                .when_some(
                    hover.zip(tile.disabled.clone()),
                    |d, ((state, shown), reason)| {
                        d.relative()
                            .on_hover(hover_setter(state))
                            .children(shown.then(|| reason_note(&element_id, reason, p)))
                    },
                )
                .when(!off, |d| {
                    d.on_click(move |_, window, cx| on_pick(key, window, cx))
                })
                .child(
                    div()
                        .flex()
                        .flex_shrink_0()
                        .items_center()
                        .justify_center()
                        .size(px(36.))
                        .rounded(px(9.))
                        .bg(tile_bg)
                        .shadow(vec![inset_ring(tile_ring, 1.)])
                        .when(off, |d| d.opacity(0.5))
                        .child(icon(tile.icon, 20., glyph)),
                )
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .min_w_0()
                        .gap(px(1.))
                        .when(off, |d| d.opacity(0.5))
                        .child(
                            styled(size::BODY, p.text)
                                .font_weight(FontWeight::SEMIBOLD)
                                .child(tile.title.clone()),
                        )
                        .child(
                            styled(size::SMALL, p.secondary)
                                .truncate()
                                .child(tile.line.clone()),
                        ),
                )
        }))
}

/// The tone of a [`badge`], [`callout`] or [`icon_tile`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    Neutral,
    Green,
    Error,
}

impl Tone {
    fn colors(self, p: &Palette) -> (Hsla, Hsla, Hsla) {
        match self {
            Tone::Neutral => (p.chip, p.chip_border, p.secondary),
            Tone::Green => (p.green_tint, p.green_border, p.green_text),
            Tone::Error => (p.error_tint, p.error_border, p.error),
        }
    }
}

/// A small pill, such as a format ("WEBP") or a count.
pub fn badge(label: impl Into<SharedString>, tone: Tone, p: &Palette) -> Div {
    let (bg, border, fg) = tone.colors(p);
    div()
        .flex()
        .flex_shrink_0()
        .items_center()
        .h(px(18.))
        .px(px(6.))
        .rounded(px(radius::SM))
        .bg(bg)
        .shadow(vec![inset_ring(border, 1.)])
        .child(
            mono(10.5, 14., fg)
                .font_weight(FontWeight::MEDIUM)
                .whitespace_nowrap()
                .child(label.into()),
        )
}

/// An icon on a tinted square, the lead of a callout or card.
pub fn icon_tile(name: IconName, tone: Tone, size: f32, p: &Palette) -> Div {
    let (bg, border, fg) = tone.colors(p);
    div()
        .flex()
        .flex_shrink_0()
        .items_center()
        .justify_center()
        .size(px(size))
        .rounded(px((size / 4.).round()))
        .bg(bg)
        .shadow(vec![inset_ring(border, 1.)])
        .child(icon(name, (size * 0.5).round(), fg))
}

/// A card on the window: the surface color, a border and soft corners.
pub fn card(p: &Palette) -> Div {
    div()
        .flex()
        .flex_col()
        .rounded(px(radius::CARD))
        .bg(p.surface)
        .border_1()
        .border_color(p.border)
        .when(!p.dark, |d| d.shadow(raise(p)))
}

/// Rows in one card, with dividers between them, like grouped settings.
pub fn group(rows: impl IntoIterator<Item = AnyElement>, p: &Palette) -> Div {
    let mut out: Vec<AnyElement> = Vec::new();
    for (i, row) in rows.into_iter().enumerate() {
        if i > 0 {
            out.push(
                div()
                    .h(px(1.))
                    .mx(px(space::LG))
                    .bg(p.row_divider)
                    .into_any_element(),
            );
        }
        out.push(row);
    }
    card(p).overflow_hidden().children(out)
}

/// A row of a [`group`]: a title, an optional description under it, and the
/// control on the right.
pub fn row(
    title: impl Into<SharedString>,
    detail: Option<AnyElement>,
    control: impl IntoElement,
    p: &Palette,
) -> Div {
    div()
        .flex()
        .items_center()
        .gap(px(space::LG))
        .min_h(px(52.))
        .px(px(space::LG))
        .py(px(10.))
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
                        .child(title.into()),
                )
                .children(detail),
        )
        .child(div().flex().flex_shrink_0().items_center().child(control))
}

/// A description under a row title.
pub fn detail(line: impl Into<SharedString>, p: &Palette) -> AnyElement {
    styled(size::SMALL, p.secondary)
        .child(line.into())
        .into_any_element()
}

/// The small label over a group or a list section.
pub fn section_label(label: impl Into<SharedString>, p: &Palette) -> Div {
    styled(size::SMALL, p.secondary)
        .font_weight(FontWeight::MEDIUM)
        .px(px(2.))
        .pb(px(space::SM))
        .child(label.into())
}

/// A message that needs attention: an icon tile, then `content` (the
/// caller's title, words and actions).
pub fn callout(name: IconName, tone: Tone, content: impl IntoElement, p: &Palette) -> Div {
    let (bg, border) = match tone {
        Tone::Neutral => (p.surface, p.border),
        Tone::Green => (p.green_tint, p.green_border),
        Tone::Error => (p.error_tint, p.error_border),
    };
    div()
        .flex()
        .items_start()
        .gap(px(space::MD))
        .p(px(14.))
        .rounded(px(radius::CARD))
        .bg(bg)
        .border_1()
        .border_color(border)
        .child(icon_tile(
            name,
            if tone == Tone::Neutral {
                Tone::Green
            } else {
                tone
            },
            28.,
            p,
        ))
        .child(div().flex().flex_col().flex_1().min_w_0().child(content))
}

/// A callout's title and body, stacked.
pub fn callout_words(
    title: impl Into<SharedString>,
    body: impl Into<SharedString>,
    p: &Palette,
) -> Div {
    div()
        .flex()
        .flex_col()
        .gap(px(2.))
        .child(
            styled(size::BODY, p.text)
                .font_weight(FontWeight::SEMIBOLD)
                .child(title.into()),
        )
        .child(styled(size::SMALL, p.secondary).child(body.into()))
}

/// The convt mark: the file you have (ink) overlapping the file you get
/// (green), with the overlap in a third shade. Drawn from the brand SVG's
/// 32-unit grid so it stays crisp at any size.
pub fn mark(size: f32, p: &Palette) -> Div {
    let k = size / 32.;
    let (ink, top, bottom, overlap) = if p.dark {
        (c(0xEDEFEE), c(0x46D08B), c(0x1FA463), c(0xA6F0C8))
    } else {
        (c(0x0A0A0A), c(0x1FB36C), c(0x127A47), c(0x0B5C34))
    };
    let square = |x: f32| {
        div()
            .absolute()
            .left(px(x * k))
            .top(px(x * k))
            .size(px(19. * k))
            .rounded(px(5. * k))
    };
    div()
        .relative()
        .flex_shrink_0()
        .size(px(size))
        .child(square(2.).bg(ink))
        .child(square(11.).bg(linear_gradient(
            180.,
            linear_color_stop(top, 0.),
            linear_color_stop(bottom, 1.),
        )))
        .child(
            div()
                .absolute()
                .left(px(11. * k))
                .top(px(11. * k))
                .size(px(10. * k))
                .rounded_tl(px(5. * k))
                .rounded_br(px(5. * k))
                .bg(overlap),
        )
}

/// The mark and the "convt" wordmark, as in the brand lockup: the mark about
/// 1.2 times the text size, half the text size apart.
pub fn lockup(text_size: f32, p: &Palette) -> Div {
    div()
        .flex()
        .items_center()
        .gap(px(text_size / 2.))
        .child(mark((text_size * 1.25).round(), p))
        .child(
            text(text_size, text_size + 4., p.text)
                .font_weight(FontWeight::SEMIBOLD)
                .child("convt"),
        )
}

/// A thumbnail for a file: the image itself for pictures, a frame for video
/// on a local disk (see `thumbs.rs`), and a colored badge with the extension
/// for everything else, including pictures that can't be decoded and videos
/// whose frame isn't ready.
pub fn thumbnail(path: &std::path::Path, w: f32, h: f32, p: &Palette) -> AnyElement {
    use convt_core::Category;
    let format = convt_core::format_by_extension(path);
    let category = format.map(|f| f.category);
    let corner = (w.min(h) / 5.).clamp(5., 10.);
    let framed = |inner: AnyElement| {
        div()
            .flex_shrink_0()
            .w(px(w))
            .h(px(h))
            .rounded(px(corner))
            .overflow_hidden()
            .bg(p.recessed)
            .child(inner)
            .child(
                // The ring sits over the picture, so light images keep an edge.
                div()
                    .absolute()
                    .inset_0()
                    .rounded(px(corner))
                    .shadow(vec![inset_ring(p.thumb_border, 1.)]),
            )
            .relative()
            .into_any_element()
    };
    match category {
        Some(Category::Image) if path.exists() => {
            let (path, p) = (path.to_path_buf(), *p);
            framed(
                img(path.clone())
                    .w(px(w))
                    .h(px(h))
                    .object_fit(ObjectFit::Cover)
                    .with_fallback(move || badge_tile(&path, category, w, h, &p))
                    .into_any_element(),
            )
        }
        Some(Category::Video) => match crate::thumbs::video_frame(path) {
            Some(frame) => {
                let (path, p) = (path.to_path_buf(), *p);
                framed(
                    img(frame)
                        .w(px(w))
                        .h(px(h))
                        .object_fit(ObjectFit::Cover)
                        .with_fallback(move || badge_tile(&path, category, w, h, &p))
                        .into_any_element(),
                )
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
    let corner = (w.min(h) / 5.).clamp(5., 10.);
    div()
        .flex()
        .items_center()
        .justify_center()
        .w(px(w))
        .h(px(h))
        .flex_shrink_0()
        .rounded(px(corner))
        .bg(p.thumb)
        .shadow(vec![inset_ring(p.thumb_border, 1.)])
        .child(
            div().px(px(4.)).py(px(1.)).rounded(px(3.)).bg(badge).child(
                mono(if w >= 48. { 9. } else { 8. }, 11., c(0xFFFFFF))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(ext.chars().take(4).collect::<String>()),
            ),
        )
        .into_any_element()
}

/// A title bar drawn in the window, for macOS, where windows use a
/// transparent title bar. Other platforms keep their native title bar, so
/// this draws nothing there.
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

/// "This Mac" or "This computer", for a label.
pub fn this_machine_label() -> &'static str {
    if cfg!(target_os = "macos") {
        "This Mac"
    } else {
        "This computer"
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

#[cfg(test)]
mod contrast {
    use super::{Hsla, PRIMARY_FILL, Palette, c};

    fn luminance(color: Hsla) -> f32 {
        let c = color.to_rgb();
        let f = |v: f32| {
            if v <= 0.03928 {
                v / 12.92
            } else {
                ((v + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * f(c.r) + 0.7152 * f(c.g) + 0.0722 * f(c.b)
    }

    fn ratio(a: Hsla, b: Hsla) -> f32 {
        let (a, b) = (luminance(a), luminance(b));
        (a.max(b) + 0.05) / (a.min(b) + 0.05)
    }

    /// Every text color reaches WCAG AA (4.5:1) on every background it is
    /// drawn on, in both appearances.
    #[test]
    fn text_meets_aa() {
        for p in [Palette::light(), Palette::dark()] {
            let grounds = [
                ("window", p.window),
                ("chrome", p.chrome),
                ("recessed", p.recessed),
                ("surface", p.surface),
                ("hover", p.hover),
                ("chip", p.chip),
                ("overlay", p.overlay),
                ("green_tint", p.green_tint),
                ("error_tint", p.error_tint),
            ];
            let inks = [
                ("text", p.text),
                ("secondary", p.secondary),
                ("tertiary", p.tertiary),
                ("green_text", p.green_text),
                ("error", p.error),
            ];
            for (ink, fg) in inks {
                for (ground, bg) in grounds {
                    let r = ratio(fg, bg);
                    assert!(r >= 4.5, "{ink} on {ground} is {r:.2}:1 (dark: {})", p.dark);
                }
            }
        }
    }

    #[test]
    fn white_on_primary_meets_aa() {
        for fill in [PRIMARY_FILL.0, PRIMARY_FILL.1] {
            assert!(ratio(c(0xFFFFFF), c(fill)) >= 4.5, "{fill:06X}");
        }
    }
}
