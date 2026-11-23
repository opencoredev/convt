//! The convt look: one visual system for every window. It holds the light and
//! dark palettes, the bundled Inter and Geist Mono fonts, the Hugeicons set,
//! the type and spacing scale, and the controls the windows share. The theme
//! follows the system appearance.
//!
//! The system, in short: cool graphite neutrals with one accent (the brand
//! green), machined controls (8px corners, a hairline ring, a top highlight
//! and a soft drop), concentric corners (a 12px track holds 8px segments 4px
//! in), Inter for words and Geist Mono for file facts (sizes, formats, paths).
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

/// Corner radii, in pixels. They nest: a control 4px inside a card has the
/// card's radius less 4.
pub mod radius {
    /// Checkboxes and small marks.
    pub const XS: f32 = 4.;
    /// Badges and menu items.
    pub const SM: f32 = 6.;
    /// Buttons, text fields, selects and segments.
    pub const CONTROL: f32 = 8.;
    /// Cards, groups, format choices and segment tracks.
    pub const CARD: f32 = 12.;
    /// Choice tiles, drop zones and the larger cards.
    pub const PANEL: f32 = 16.;
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
    /// A check mark on a `green` fill: white on the deep light-mode green,
    /// ink on the bright dark-mode one.
    pub on_green: Hsla,
    pub green_tint: Hsla,
    pub green_border: Hsla,
    pub error: Hsla,
    pub error_tint: Hsla,
    pub error_border: Hsla,
    /// Advisories, such as a photo that grows as PNG: text and icons, at
    /// least 4.5:1 on every ground.
    pub amber: Hsla,
    pub amber_tint: Hsla,
    pub amber_border: Hsla,
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

/// Graphical UI Alloy's cool graphite neutrals with the convt green. The
/// accent, error and amber inks each reach 4.5:1 on every ground they are
/// drawn on (see `contrast`).
impl Palette {
    pub fn light() -> Self {
        Self {
            dark: false,
            window: c(0xFFFFFF),
            chrome: c(0xFAFAFA),
            chrome_border: c(0xE4E4E7),
            recessed: c(0xF2F2F3),
            surface: c(0xFFFFFF),
            border: c(0xE4E4E7),
            control_border: c(0xD4D4D8),
            hairline: c(0xE4E4E7),
            row_divider: c(0xEFEFF1),
            chip: c(0xF2F2F3),
            chip_border: c(0xE4E4E7),
            text: c(0x202124),
            secondary: c(0x5D5D65),
            tertiary: c(0x62626A),
            hover: c(0xF2F2F3),
            selected: c(0xE8E8EC),
            green: c(0x127A47),
            green_text: c(0x127A47),
            on_green: c(0xFFFFFF),
            green_tint: c(0xEEF7F2),
            green_border: c(0xC7E5D5),
            error: c(0xB3261E),
            error_tint: c(0xFDF2F2),
            error_border: c(0xF2CECE),
            amber: c(0x945B0A),
            amber_tint: c(0xFEF8EC),
            amber_border: c(0xF2DBAE),
            track: c(0xE4E4E7),
            toggle_off: c(0xC8C8CF),
            // Dark enough for 3:1 on every ground a checkbox sits on.
            mark_off: c(0x7E7E87),
            thumb: c(0xFFFFFF),
            thumb_border: c(0xE4E4E7),
            overlay: c(0xFFFFFF),
            shadow: ca(0x10182824),
            shadow_soft: ca(0x1018281A),
        }
    }

    pub fn dark() -> Self {
        Self {
            dark: true,
            window: c(0x171719),
            chrome: c(0x1D1D20),
            chrome_border: c(0x2C2C32),
            recessed: c(0x242428),
            surface: c(0x202023),
            border: c(0x323238),
            control_border: c(0x44444C),
            hairline: c(0x2C2C32),
            row_divider: c(0x2B2B31),
            chip: c(0x26262B),
            chip_border: c(0x3B3B42),
            text: c(0xFAFAFA),
            secondary: c(0xB2B2BC),
            tertiary: c(0x9E9EA8),
            hover: c(0x2A2A2E),
            selected: c(0x323238),
            green: c(0x3FCB84),
            green_text: c(0x3FCB84),
            on_green: c(0x171719),
            green_tint: c(0x14271D),
            green_border: ca(0x3FCB8438),
            error: c(0xF48C92),
            error_tint: c(0x2A181A),
            error_border: ca(0xF48C9238),
            amber: c(0xE8BD6B),
            amber_tint: c(0x292216),
            amber_border: ca(0xE8BD6B38),
            track: c(0x2E2E34),
            toggle_off: c(0x3B3B42),
            // Light enough for 3:1 on every ground a checkbox sits on.
            mark_off: c(0x85858F),
            thumb: c(0x242428),
            thumb_border: ca(0xFFFFFF16),
            overlay: c(0x242428),
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
    HardDrive,
    Image,
    Inbox,
    Info,
    Loader,
    Minus,
    Music,
    Pdf,
    Plus,
    Presentation,
    RefreshCw,
    RightClick,
    RotateCw,
    Settings,
    Spreadsheet,
    Star,
    TriangleAlert,
    Vector,
    Video,
    WindowMaximize,
    WindowRestore,
}

impl IconName {
    #[cfg(test)]
    pub const ALL: [IconName; 42] = {
        use IconName::*;
        [
            ArrowDown,
            ArrowRight,
            Ban,
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
            HardDrive,
            Image,
            Inbox,
            Info,
            Loader,
            Minus,
            Music,
            Pdf,
            Plus,
            Presentation,
            RefreshCw,
            RightClick,
            RotateCw,
            Settings,
            Spreadsheet,
            Star,
            TriangleAlert,
            Vector,
            Video,
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
            HardDrive => "hard-drive",
            Image => "image",
            Inbox => "inbox",
            Info => "info",
            Loader => "loading",
            Minus => "minus",
            Music => "music",
            Pdf => "pdf",
            Plus => "add",
            Presentation => "presentation",
            RefreshCw => "refresh",
            RightClick => "cursor-in-window",
            RotateCw => "rotate",
            Settings => "settings",
            Spreadsheet => "spreadsheet",
            Star => "star",
            TriangleAlert => "alert-triangle",
            Vector => "vector",
            Video => "video",
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
pub const FIELD_HEIGHT: f32 = 32.;
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

/// A line drawn inside an element's top (`y` > 0) or bottom (`y` < 0) edge:
/// the specular highlight or the shade of a machined control.
pub fn inset_edge(color: Hsla, y: f32) -> BoxShadow {
    BoxShadow {
        color,
        offset: point(px(0.), px(y)),
        blur_radius: px(0.),
        spread_radius: px(0.),
        inset: true,
    }
}

/// `--shadow-s`: the small drop under raised controls and segments.
pub fn shadow_s(p: &Palette) -> BoxShadow {
    shadow(
        if p.dark {
            ca(0x00000059)
        } else {
            ca(0x1018280F)
        },
        1.,
        2.,
    )
}

/// The resting shadow of raised controls and cards.
fn raise(p: &Palette) -> Vec<BoxShadow> {
    vec![shadow_s(p)]
}

/// The bevel of a raised control on top of its fill: a hairline ring, a
/// highlight along the top edge, a shade along the bottom and `--shadow-s`.
pub fn bevel(ring: Hsla, p: &Palette) -> Vec<BoxShadow> {
    let (top, bottom) = if p.dark {
        (ca(0xFFFFFF14), ca(0x00000040))
    } else {
        (ca(0xFFFFFFCC), ca(0x10182808))
    };
    vec![
        inset_ring(ring, 1.),
        inset_edge(top, 1.),
        inset_edge(bottom, -1.),
        shadow_s(p),
    ]
}

/// The inset top shadow of a recessed well, such as a segment track or a
/// drop zone.
pub fn well(p: &Palette) -> Vec<BoxShadow> {
    vec![
        inset_ring(p.border, 1.),
        BoxShadow {
            color: if p.dark {
                ca(0x00000040)
            } else {
                ca(0x1018280D)
            },
            offset: point(px(0.), px(1.)),
            blur_radius: px(2.),
            spread_radius: px(0.),
            inset: true,
        },
    ]
}

/// A top-to-bottom sheen from `top` to `bottom`.
fn sheen((top, bottom): (u32, u32)) -> Background {
    linear_gradient(
        180.,
        linear_color_stop(c(top), 0.),
        linear_color_stop(c(bottom), 1.),
    )
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

/// The machined green of a primary button in the light appearance, top and
/// bottom of its sheen, resting and hovered. White on each is past 4.5:1.
const GREEN_SHEEN: (u32, u32) = (0x15854E, 0x106E3F);
const GREEN_SHEEN_HOVER: (u32, u32) = (0x127A47, 0x0E6638);
/// The same in the dark appearance, under ink.
const GREEN_SHEEN_DARK: (u32, u32) = (0x48D38D, 0x34BC77);
const GREEN_SHEEN_DARK_HOVER: (u32, u32) = (0x52D896, 0x3FCB84);
/// The ring around the green: a step deeper than its fill.
const GREEN_RING: (u32, u32) = (0x0C5F36, 0x2FA868);
/// A secondary button's raised white or graphite, resting and hovered.
const RAISED_SHEEN: (u32, u32) = (0xFFFFFF, 0xF7F7F9);
const RAISED_SHEEN_HOVER: (u32, u32) = (0xFAFAFB, 0xF0F0F2);
const RAISED_SHEEN_DARK: (u32, u32) = (0x28282E, 0x222227);
const RAISED_SHEEN_DARK_HOVER: (u32, u32) = (0x2F2F36, 0x28282E);

/// How a [`Button`] looks. Every button is machined: 8px corners, a hairline
/// ring, a highlight along its top edge and a small drop. One green button
/// per window or card for its main action, raised secondaries beside it and
/// quiet ghost buttons in rows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Look {
    /// The one main action: the brand green.
    Primary,
    /// A raised white (graphite in dark mode) button, such as Cancel.
    Secondary,
    /// Text that gains a soft fill on hover, for row actions.
    Ghost,
    /// The brand green, for a purchase or a trial. Drawn as Primary.
    Brand,
}

/// A button's height.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Height {
    /// 28px, in rows, cards and the popover.
    Small,
    /// 32px, the default in windows.
    Regular,
    /// 42px, the wide onboarding buttons.
    Large,
}

/// Something drawn before a button's label.
enum Lead {
    Icon(IconName),
    Google,
}

/// A button: a look, a label, an optional leading icon and three heights.
pub struct Button {
    id: ElementId,
    label: SharedString,
    look: Look,
    lead: Option<Lead>,
    height: Height,
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
            lead: None,
            height: Height::Regular,
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

    pub fn brand(id: impl Into<ElementId>, label: impl Into<SharedString>) -> Self {
        Self::new(id, label, Look::Brand)
    }

    pub fn icon(mut self, icon: IconName) -> Self {
        self.lead = Some(Lead::Icon(icon));
        self
    }

    /// Google's four-color G before the label.
    pub fn google(mut self) -> Self {
        self.lead = Some(Lead::Google);
        self
    }

    /// 28px tall instead of 32.
    pub fn small(mut self) -> Self {
        self.height = Height::Small;
        self
    }

    /// 42px tall, as the onboarding buttons.
    pub fn large(mut self) -> Self {
        self.height = Height::Large;
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
        let (h, pad, size, line, icon_size) = match self.height {
            Height::Small => (28., 12., 12., 16., 13.),
            Height::Regular => (32., 16., 13., 16., 14.),
            Height::Large => (42., 24., 14., 18., 16.),
        };
        let (bg, hover_bg, fg): (Background, Background, Hsla) = match self.look {
            Look::Primary | Look::Brand if p.dark => (
                sheen(GREEN_SHEEN_DARK),
                sheen(GREEN_SHEEN_DARK_HOVER),
                p.on_green,
            ),
            Look::Primary | Look::Brand => {
                (sheen(GREEN_SHEEN), sheen(GREEN_SHEEN_HOVER), p.on_green)
            }
            Look::Secondary if p.dark => (
                sheen(RAISED_SHEEN_DARK),
                sheen(RAISED_SHEEN_DARK_HOVER),
                self.color.unwrap_or(p.text),
            ),
            Look::Secondary => (
                sheen(RAISED_SHEEN),
                sheen(RAISED_SHEEN_HOVER),
                self.color.unwrap_or(p.text),
            ),
            Look::Ghost => (
                transparent_black().into(),
                p.hover.into(),
                self.color.unwrap_or(p.secondary),
            ),
        };
        let hover_fg = match self.look {
            Look::Ghost => self.color.unwrap_or(p.text),
            _ => fg,
        };
        let still = self.disabled || self.loading;
        let base = clickable(self.id, self.label.clone())
            .flex()
            .flex_shrink_0()
            .items_center()
            .justify_center()
            .gap(px(if self.height == Height::Large {
                10.
            } else {
                6.
            }))
            .h(px(h))
            .px(px(if self.look == Look::Ghost {
                pad - 4.
            } else {
                pad
            }))
            .rounded(px(radius::CONTROL))
            .bg(bg)
            .when(!still, |d| d.hover(move |s| s.bg(hover_bg)));
        let base = match self.look {
            Look::Primary | Look::Brand => {
                let ring = c(if p.dark { GREEN_RING.1 } else { GREEN_RING.0 });
                let mut s = bevel(ring, p);
                if self.height == Height::Large {
                    s.push(shadow(p.shadow_soft, 2., 8.));
                }
                base.shadow(s)
            }
            Look::Secondary => base.shadow(bevel(p.control_border, p)),
            Look::Ghost => base,
        };
        let icon_color = match self.look {
            Look::Secondary | Look::Ghost if self.color.is_none() => p.secondary,
            _ => fg,
        };
        let lead = match self.lead {
            Some(Lead::Google) => Some(google_mark(icon_size).into_any_element()),
            Some(Lead::Icon(i)) => Some(icon(i, icon_size, icon_color).into_any_element()),
            None => None,
        };
        base.when(self.disabled, |d| d.opacity(0.45))
            .when(still, |d| d.cursor_default())
            .when(self.loading, |d| {
                d.child(Spinner::new().with_size(px(icon_size)).color(icon_color))
            })
            .when(!self.loading, |d| d.children(lead))
            .child(
                text(size, line, fg)
                    .font_weight(FontWeight::MEDIUM)
                    .whitespace_nowrap()
                    .when(self.look == Look::Ghost && !still, |d| {
                        d.hover(|s| s.text_color(hover_fg))
                    })
                    .child(self.label),
            )
    }
}

/// The green button, 32px tall: the one main action.
pub fn primary_button(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    p: &Palette,
) -> Clickable {
    Button::primary(id, label).build(p)
}

/// The raised secondary button, such as Cancel.
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
    clickable(id, label.clone())
        // Underlined rather than faded, so hovered text keeps its contrast.
        .hover(|s| s.underline())
        .child(
            text(size, 16., color)
                .font_weight(FontWeight::MEDIUM)
                .whitespace_nowrap()
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
        .shadow(switch_track_shadow(on, p))
        .when(on, |d| d.justify_end())
        .child(
            div()
                .size(px(knob))
                .rounded(px(knob / 2.))
                .bg(c(0xFFFFFF))
                .shadow(vec![
                    inset_ring(ca(0x0000000F), 0.5),
                    shadow(ca(0x00000033), 1., 2.),
                    shadow_s(p),
                ]),
        )
}

/// The switch track's shadows. Both states are recessed: a soft shade along
/// the top edge. An off track also gets a 1px `mark_off` ring, so its edge
/// meets 3:1 against the card or window it sits on even though its fill
/// does not.
fn switch_track_shadow(on: bool, p: &Palette) -> Vec<BoxShadow> {
    let recess = BoxShadow {
        color: if p.dark {
            ca(0x00000040)
        } else {
            ca(0x2021241A)
        },
        offset: point(px(0.), px(1.)),
        blur_radius: px(2.),
        spread_radius: px(0.),
        inset: true,
    };
    if on {
        vec![recess]
    } else {
        vec![inset_ring(p.mark_off, 1.), recess]
    }
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
            .rounded(px(radius::XS))
            .bg(p.green)
            .child(icon(IconName::Check, 11., p.on_green))
    } else {
        div()
            .size(px(15.))
            .rounded(px(radius::XS))
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
/// `open`, it lists `choices` below, each with the id `{id}-{choice id}`,
/// and checks the one whose id is `selected`. The shown value can differ
/// from the pick: an unset Background shows the default it converts with.
#[allow(clippy::too_many_arguments)]
pub fn select(
    id: &str,
    value: impl Into<SharedString>,
    selected: &str,
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
            let on = choice.id.as_ref() == selected;
            clickable(
                SharedString::from(format!("{id}-{}", choice.id)),
                choice.label.clone(),
            )
            .aria_selected(on)
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
    pill_track(p).children(choices.iter().map(|(key, label)| {
        let key = *key;
        let on = key == selected;
        let on_pick = on_pick.clone();
        clickable(SharedString::from(format!("{id}-{key}")), label.clone())
            .aria_selected(on)
            .flex()
            .items_center()
            .h(px(24.))
            .px(px(12.))
            .rounded(px(radius::CONTROL))
            .when(on, |d| pill_segment_on(d, p))
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

/// The recessed track that holds a segmented control or tabs: 12px corners
/// around 8px segments, 4px in, so the corners nest.
pub fn pill_track(p: &Palette) -> Div {
    div()
        .flex()
        .flex_shrink_0()
        .p(px(4.))
        .gap(px(2.))
        .rounded(px(radius::CARD))
        .bg(p.recessed)
        .shadow(well(p))
}

/// The picked segment of a [`pill_track`]: a raised, beveled surface.
pub fn pill_segment_on(d: Clickable, p: &Palette) -> Clickable {
    d.bg(segment_fill(p)).shadow(bevel(p.border, p))
}

/// The picked segment's fill.
fn segment_fill(p: &Palette) -> Hsla {
    if p.dark { p.selected } else { p.surface }
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

/// The look of anything picked from a set (format cards, presets, the
/// onboarding questions): a quiet surface with a hairline ring and the
/// container lift of [`soft`], and for the picked one a crisp 1.5px ring in
/// the brand green on a green tint, with no drop of its own. The ring is
/// also what the keyboard moves.
pub fn choice(el: Clickable, on: bool, corner: f32, p: &Palette) -> Clickable {
    el.rounded(px(corner))
        .bg(if on { p.green_tint } else { p.surface })
        .shadow(choice_shadow(on, p))
        .when(!on, |d| d.hover(|s| s.bg(p.hover)))
}

fn choice_shadow(on: bool, p: &Palette) -> Vec<BoxShadow> {
    if on {
        vec![inset_ring(p.green, 1.5)]
    } else {
        let mut s = vec![inset_ring(p.border, 1.)];
        s.extend(soft(p));
        s
    }
}

/// A large square choice with an icon over its label, as onboarding asks
/// its questions.
pub fn choice_tile(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    glyph: IconName,
    on: bool,
    p: &Palette,
) -> Clickable {
    let label = label.into();
    choice(clickable(id, label.clone()), on, radius::PANEL, p)
        .aria_selected(on)
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .gap(px(12.))
        .w(px(176.))
        .h(px(136.))
        .child(icon(glyph, 30., if on { p.green } else { p.secondary }))
        .child(
            styled(size::BODY, p.text)
                .font_weight(FontWeight::SEMIBOLD)
                .child(label),
        )
}

/// One choice of [`segmented_with`].
pub struct Segment {
    pub key: &'static str,
    pub icon: Option<IconName>,
    pub label: SharedString,
    /// Why the choice can't be picked now; shown above it on hover or click.
    pub disabled: Option<SharedString>,
}

/// [`segmented`] with an icon per choice and choices that can't be picked
/// now, such as where a conversion runs. A disabled segment keeps its label
/// readable (the tertiary ink, at least 4.5:1 on the track), ignores picks
/// and says why in a note above it while hovered or after a click, so the
/// note never covers the label it explains (GPUI's own tooltip follows the
/// pointer and, near a window's bottom edge, lands on the control). Ids are
/// `{id}-{key}`; the note is `{id}-{key}-reason`.
pub fn segmented_with(
    id: &str,
    choices: &[Segment],
    selected: &str,
    p: &Palette,
    window: &mut Window,
    cx: &mut App,
    on_pick: impl Fn(&'static str, &mut Window, &mut App) + Clone + 'static,
) -> Div {
    pill_track(p).children(choices.iter().map(|segment| {
        let key = segment.key;
        let on = key == selected && segment.disabled.is_none();
        let element_id = SharedString::from(format!("{id}-{key}"));
        let ink = if segment.disabled.is_some() {
            p.tertiary
        } else if on {
            p.text
        } else {
            p.secondary
        };
        let el = clickable(element_id.clone(), segment.label.clone())
            .aria_selected(on)
            .flex()
            .items_center()
            .gap(px(6.))
            .h(px(24.))
            .px(px(10.))
            .rounded(px(radius::CONTROL))
            .when(on, |d| pill_segment_on(d, p))
            .children(segment.icon.map(|glyph| icon(glyph, 13., ink)))
            .child(
                text(12., 16., ink)
                    .font_weight(FontWeight::MEDIUM)
                    .whitespace_nowrap()
                    .child(segment.label.clone()),
            );
        match &segment.disabled {
            None => {
                let on_pick = on_pick.clone();
                el.when(!on, |d| d.hover(|s| s.bg(p.hover)))
                    .on_click(move |_, window, cx| on_pick(key, window, cx))
            }
            Some(reason) => {
                let state = window.use_keyed_state(
                    SharedString::from(format!("{element_id}-hover")),
                    cx,
                    |_, _| false,
                );
                let shown = *state.read(cx);
                let click = state.clone();
                el.cursor_default()
                    .relative()
                    .on_hover(hover_setter(state))
                    .on_click(move |_, _, cx| {
                        click.update(cx, |s, cx| {
                            *s = true;
                            cx.notify();
                        })
                    })
                    .children(shown.then(|| reason_note(&element_id, reason.clone(), p)))
            }
        }
    }))
}

/// The tone of a [`badge`], [`callout`] or [`icon_tile`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    Neutral,
    Green,
    Error,
    /// A warm advisory, such as a file that grows a lot.
    Amber,
}

impl Tone {
    fn colors(self, p: &Palette) -> (Hsla, Hsla, Hsla) {
        match self {
            Tone::Neutral => (p.chip, p.chip_border, p.secondary),
            Tone::Green => (p.green_tint, p.green_border, p.green_text),
            Tone::Error => (p.error_tint, p.error_border, p.error),
            Tone::Amber => (p.amber_tint, p.amber_border, p.amber),
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

/// A card on the window: the surface color, a hairline border, 12px
/// corners and the calm container lift of [`soft`].
pub fn card(p: &Palette) -> Div {
    div()
        .flex()
        .flex_col()
        .rounded(px(radius::CARD))
        .bg(p.surface)
        .border_1()
        .border_color(p.border)
        .shadow(soft(p))
}

/// The resting shadow of calm containers (cards, groups, unpicked choices):
/// in light mode a 1px hairline lift with no blur spread, in dark mode
/// nothing, since the hairline border does the work. Raised controls
/// (buttons, selects, the picked segment) keep [`bevel`] and `--shadow-s`,
/// so they stand off these containers.
pub fn soft(p: &Palette) -> Vec<BoxShadow> {
    if p.dark {
        Vec::new()
    } else {
        vec![shadow(ca(0x20212408), 1., 1.)]
    }
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
        Tone::Amber => (p.amber_tint, p.amber_border),
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

/// The onboarding's dithered green glow (`assets/onboarding`), rising from
/// the bottom of whatever holds it, at `strength` (0 to 1) of its full
/// opacity, pushed `sink` pixels below the holder's bottom edge so a short
/// holder shows only its faint upper reach and never a cut-off band. The
/// holder needs `relative()` and `overflow_hidden()`. Used
/// sparingly, where a quiet moment can take some warmth: onboarding, the
/// empty Activity page, the License tab's status and the popover's header.
pub fn glow(strength: f32, sink: f32, p: &Palette) -> Img {
    img(SharedString::from(format!(
        "onboarding/glow-{}.png",
        if p.dark { "dark" } else { "light" }
    )))
    .absolute()
    .bottom(px(-sink))
    .left(relative(0.5))
    .ml(px(-GLOW.0 / 2.))
    .w(px(GLOW.0))
    .h(px(GLOW.1))
    .opacity(strength)
}

/// The glow PNGs' pixel size, drawn unscaled so the cells stay square.
pub const GLOW: (f32, f32) = (2400., 540.);

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
                // Rounded itself: GPUI draws an image's square corners over
                // a parent's rounded clip.
                img(path.clone())
                    .w(px(w))
                    .h(px(h))
                    .rounded(px(corner))
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
                        .rounded(px(corner))
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
    use super::{
        GREEN_SHEEN, GREEN_SHEEN_DARK, GREEN_SHEEN_DARK_HOVER, GREEN_SHEEN_HOVER, Hsla, Palette,
        RAISED_SHEEN, RAISED_SHEEN_DARK, RAISED_SHEEN_DARK_HOVER, RAISED_SHEEN_HOVER, c,
        segment_fill,
    };

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
                ("amber_tint", p.amber_tint),
            ];
            let inks = [
                ("text", p.text),
                ("secondary", p.secondary),
                ("tertiary", p.tertiary),
                ("green_text", p.green_text),
                ("error", p.error),
                ("amber", p.amber),
            ];
            for (ink, fg) in inks {
                for (ground, bg) in grounds {
                    let r = ratio(fg, bg);
                    assert!(r >= 4.5, "{ink} on {ground} is {r:.2}:1 (dark: {})", p.dark);
                }
            }
        }
    }

    /// Control marks need 3:1: a check against its fill, an empty box's
    /// edge against the ground it sits on.
    #[test]
    fn control_marks_meet_three_to_one() {
        for p in [Palette::light(), Palette::dark()] {
            let r = ratio(p.on_green, p.green);
            assert!(r >= 3., "check on green is {r:.2}:1 (dark: {})", p.dark);
            for (ground, bg) in [
                ("window", p.window),
                ("chrome", p.chrome),
                ("recessed", p.recessed),
                ("surface", p.surface),
                ("hover", p.hover),
                ("overlay", p.overlay),
            ] {
                let r = ratio(p.mark_off, bg);
                assert!(
                    r >= 3.,
                    "mark_off on {ground} is {r:.2}:1 (dark: {})",
                    p.dark
                );
            }
        }
    }

    /// An off switch's track is outlined in `mark_off`, which carries its
    /// 3:1 boundary on cards and windows; its fill alone does not.
    #[test]
    fn inactive_switch_ring_meets_three_to_one() {
        for p in [Palette::light(), Palette::dark()] {
            let shadows = super::switch_track_shadow(false, &p);
            let ring = &shadows[0];
            assert!(ring.inset && ring.blur_radius == super::px(0.));
            assert_eq!(ring.color, p.mark_off);
            for (ground, bg) in [("surface", p.surface), ("window", p.window)] {
                let r = ratio(ring.color, bg);
                assert!(
                    r >= 3.,
                    "off switch ring on {ground} is {r:.2}:1 (dark: {})",
                    p.dark
                );
            }
        }
    }

    /// Unpicked tabs and segments draw `secondary` on the recessed track
    /// and on their hover; the picked one draws `text` on its raised fill.
    #[test]
    fn unpicked_segments_meet_aa() {
        for p in [Palette::light(), Palette::dark()] {
            assert!(ratio(p.secondary, p.recessed) >= 4.5, "dark: {}", p.dark);
            assert!(ratio(p.secondary, p.hover) >= 4.5, "dark: {}", p.dark);
            assert!(ratio(p.text, segment_fill(&p)) >= 4.5, "dark: {}", p.dark);
        }
    }

    /// Every button's label against both ends of its sheen, resting and
    /// hovered.
    #[test]
    fn button_labels_meet_aa() {
        let (light, dark) = (Palette::light(), Palette::dark());
        for (label, sheens) in [
            (light.on_green, [GREEN_SHEEN, GREEN_SHEEN_HOVER]),
            (dark.on_green, [GREEN_SHEEN_DARK, GREEN_SHEEN_DARK_HOVER]),
            (light.text, [RAISED_SHEEN, RAISED_SHEEN_HOVER]),
            (dark.text, [RAISED_SHEEN_DARK, RAISED_SHEEN_DARK_HOVER]),
        ] {
            for (top, bottom) in sheens {
                for fill in [top, bottom] {
                    let r = ratio(label, c(fill));
                    assert!(r >= 4.5, "label on {fill:06X} is {r:.2}:1");
                }
            }
        }
        // Ghost buttons draw `secondary`, then `text` on the hover fill.
        for p in [light, dark] {
            assert!(ratio(p.text, p.hover) >= 4.5);
        }
    }
}
