//! Where the System Settings window is, and moving the Finder guide's panel
//! beside it (`ui::finder_guide`).
//!
//! The window server reports every app's window bounds without Screen
//! Recording or Accessibility access, so the guide asks for no permission of
//! its own. It only reads System Settings' bounds and whether it is the
//! frontmost app; it never looks inside the window.

use std::ffi::c_void;

use gpui_kit::Window;
use objc::runtime::{BOOL, NO, Object, YES};
use objc::{class, msg_send, sel, sel_impl};
use raw_window_handle::{HasWindowHandle, RawWindowHandle};

use crate::finder::Frame;

const BUNDLE_ID: &str = "com.apple.systempreferences";

/// `kCGWindowListOptionOnScreenOnly | kCGWindowListExcludeDesktopElements`.
const ON_SCREEN: u32 = (1 << 0) | (1 << 4);
/// `NSFloatingWindowLevel`: above other apps' windows, below menus.
const FLOATING_LEVEL: i64 = 3;

#[repr(C)]
#[derive(Clone, Copy)]
struct NSPoint {
    x: f64,
    y: f64,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct NSSize {
    w: f64,
    h: f64,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct NSRect {
    origin: NSPoint,
    size: NSSize,
}

#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
    /// Returns a `CFArrayRef` of `CFDictionaryRef`s, toll-free bridged to
    /// `NSArray` and `NSDictionary`.
    fn CGWindowListCopyWindowInfo(option: u32, relative_to: u32) -> *mut Object;
}

#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    fn CFRelease(cf: *const c_void);
}

/// What System Settings is showing.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Settings {
    NotRunning,
    /// Running, with no window on screen: hidden, minimized, or still
    /// opening.
    NoWindow,
    Window(Frame),
}

fn ns_string(s: &str) -> *mut Object {
    let c = std::ffi::CString::new(s).expect("no NUL");
    // SAFETY: a valid C string; the result is autoreleased.
    unsafe { msg_send![class!(NSString), stringWithUTF8String: c.as_ptr()] }
}

/// System Settings' main window: its largest normal window on screen (a
/// sheet is a separate, smaller window on top of it). System Settings has
/// one main window.
pub fn window() -> Settings {
    objc::rc::autoreleasepool(window_now)
}

fn window_now() -> Settings {
    // SAFETY: Foundation and CoreGraphics calls on the main thread; every
    // object is checked for nil, and the copied array is released.
    unsafe {
        let apps: *mut Object = msg_send![
            class!(NSRunningApplication),
            runningApplicationsWithBundleIdentifier: ns_string(BUNDLE_ID)
        ];
        let app: *mut Object = msg_send![apps, firstObject];
        if app.is_null() {
            return Settings::NotRunning;
        }
        let pid: i32 = msg_send![app, processIdentifier];
        let list = CGWindowListCopyWindowInfo(ON_SCREEN, 0);
        if list.is_null() {
            return Settings::NoWindow;
        }
        let (owner, layer, bounds) = (
            ns_string("kCGWindowOwnerPID"),
            ns_string("kCGWindowLayer"),
            ns_string("kCGWindowBounds"),
        );
        let number = |dict: *mut Object, key: *mut Object| -> Option<f64> {
            let n: *mut Object = msg_send![dict, objectForKey: key];
            (!n.is_null()).then(|| msg_send![n, doubleValue])
        };
        let count: usize = msg_send![list, count];
        let mut best: Option<Frame> = None;
        for i in 0..count {
            let info: *mut Object = msg_send![list, objectAtIndex: i];
            if number(info, owner) != Some(pid as f64) || number(info, layer) != Some(0.) {
                continue;
            }
            let b: *mut Object = msg_send![info, objectForKey: bounds];
            if b.is_null() {
                continue;
            }
            let (Some(x), Some(y), Some(w), Some(h)) = (
                number(b, ns_string("X")),
                number(b, ns_string("Y")),
                number(b, ns_string("Width")),
                number(b, ns_string("Height")),
            ) else {
                continue;
            };
            let frame = Frame { x, y, w, h };
            if w >= 200. && best.is_none_or(|prev| w * h > prev.w * prev.h) {
                best = Some(frame);
            }
        }
        CFRelease(list as *const c_void);
        best.map_or(Settings::NoWindow, Settings::Window)
    }
}

/// Whether this macOS lists extensions at the bottom of Login Items &
/// Extensions, as the guide draws them (macOS 15 and later). Earlier
/// versions keep them under Privacy & Security.
pub fn lists_extensions_in_login_items() -> bool {
    #[repr(C)]
    struct Version {
        major: i64,
        _minor: i64,
        _patch: i64,
    }
    // SAFETY: a Foundation call returning a plain struct.
    let version: Version = unsafe {
        let info: *mut Object = msg_send![class!(NSProcessInfo), processInfo];
        msg_send![info, operatingSystemVersion]
    };
    version.major >= 15
}

/// Whether System Settings is the app the user is in.
pub fn frontmost() -> bool {
    objc::rc::autoreleasepool(frontmost_now)
}

fn frontmost_now() -> bool {
    // SAFETY: Foundation calls on valid objects, checked for nil.
    unsafe {
        let workspace: *mut Object = msg_send![class!(NSWorkspace), sharedWorkspace];
        let app: *mut Object = msg_send![workspace, frontmostApplication];
        if app.is_null() {
            return false;
        }
        let id: *mut Object = msg_send![app, bundleIdentifier];
        if id.is_null() {
            return false;
        }
        let same: BOOL = msg_send![id, isEqualToString: ns_string(BUNDLE_ID)];
        same == YES
    }
}

/// The height of the primary display, which AppKit's bottom-left origin is
/// measured from.
unsafe fn primary_height() -> f64 {
    unsafe {
        let screens: *mut Object = msg_send![class!(NSScreen), screens];
        let primary: *mut Object = msg_send![screens, firstObject];
        if primary.is_null() {
            return 0.;
        }
        let frame: NSRect = msg_send![primary, frame];
        frame.size.h
    }
}

/// The usable area of every display (no menu bar or Dock), measured like
/// [`window`].
pub fn screens() -> Vec<Frame> {
    objc::rc::autoreleasepool(screens_now)
}

fn screens_now() -> Vec<Frame> {
    // SAFETY: AppKit calls on the main thread.
    unsafe {
        let top = primary_height();
        let screens: *mut Object = msg_send![class!(NSScreen), screens];
        let count: usize = msg_send![screens, count];
        (0..count)
            .map(|i| {
                let screen: *mut Object = msg_send![screens, objectAtIndex: i];
                let f: NSRect = msg_send![screen, visibleFrame];
                Frame {
                    x: f.origin.x,
                    y: top - (f.origin.y + f.size.h),
                    w: f.size.w,
                    h: f.size.h,
                }
            })
            .collect()
    }
}

/// The guide's own `NSWindow`. AppKit calls back into GPUI while a window
/// moves or orders front, so these run between GPUI updates, never inside
/// one.
pub struct Panel(*mut Object);

/// The `NSWindow` behind a GPUI window.
pub fn panel(window: &Window) -> Option<Panel> {
    let RawWindowHandle::AppKit(handle) = HasWindowHandle::window_handle(window).ok()?.as_raw()
    else {
        return None;
    };
    let view = handle.ns_view.as_ptr() as *mut Object;
    // SAFETY: GPUI's live content view.
    let window: *mut Object = unsafe { msg_send![view, window] };
    (!window.is_null()).then_some(Panel(window))
}

impl Panel {
    /// Makes the panel a plain floating card: no window buttons, kept on
    /// screen while another app is active, above that app's windows.
    pub fn prepare(&self) {
        let w = self.0;
        // SAFETY: AppKit calls on the main thread on the live window.
        unsafe {
            for kind in 0..3_u64 {
                let button: *mut Object = msg_send![w, standardWindowButton: kind];
                if !button.is_null() {
                    let _: () = msg_send![button, setHidden: YES];
                }
            }
            let _: () = msg_send![w, setHidesOnDeactivate: NO];
            let _: () = msg_send![w, setLevel: FLOATING_LEVEL];
        }
    }

    /// Moves the panel's top left corner to `at` (measured like [`window`])
    /// and shows or hides it. Showing fades it in.
    pub fn place(&self, at: Option<(f64, f64)>, show: bool) {
        let w = self.0;
        // SAFETY: AppKit calls on the main thread on the live window.
        unsafe {
            if let Some((x, y)) = at {
                let point = NSPoint {
                    x,
                    y: primary_height() - y,
                };
                let _: () = msg_send![w, setFrameTopLeftPoint: point];
            }
            let visible: BOOL = msg_send![w, isVisible];
            if show && visible != YES {
                let _: () = msg_send![w, setAlphaValue: 0.0_f64];
                let _: () = msg_send![w, orderFrontRegardless];
                let animator: *mut Object = msg_send![w, animator];
                let _: () = msg_send![animator, setAlphaValue: 1.0_f64];
            } else if !show && visible == YES {
                let _: () = msg_send![w, orderOut: std::ptr::null_mut::<Object>()];
            }
        }
    }
}
