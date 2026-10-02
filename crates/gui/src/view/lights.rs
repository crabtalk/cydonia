//! The macOS traffic lights' size and spacing, read off a window's own
//! buttons. They differ across macOS releases — 14px on a 23px pitch on
//! macOS 26, 12px on a 20px pitch before it — and AppKit owns them, so the
//! header asks rather than assumes.

use crate::view::root::TRAFFIC_LIGHT_INSET;
use bezel::gpui::{App, Global, Window};

/// One light's diameter and the distance between two lights' left edges.
#[derive(Clone, Copy)]
pub struct Lights {
    pub size: f32,
    pub pitch: f32,
}

impl Global for Lights {}

/// What macOS 26 draws, for a window not measured yet and off macOS.
const MACOS_26: Lights = Lights {
    size: 14.,
    pitch: 23.,
};

impl Lights {
    /// The lights measured so far, or macOS 26's.
    pub fn of(cx: &App) -> Self {
        cx.try_global::<Self>().copied().unwrap_or(MACOS_26)
    }

    /// Where the green light's right edge lands, the close button standing at
    /// [`TRAFFIC_LIGHT_INSET`].
    pub fn end(&self) -> f32 {
        TRAFFIC_LIGHT_INSET + 2. * self.pitch + self.size
    }
}

/// Measure `window`'s lights, record them for every band to read, and centre
/// the lights down the header. Call once the window exists.
pub fn fit(window: &mut Window, cx: &mut App) {
    let lights = measure(window).unwrap_or(MACOS_26);
    cx.set_global(lights);
    // gpui has the setter on macOS only.
    #[cfg(target_os = "macos")]
    {
        use crate::view::root::HEADER_HEIGHT;
        use bezel::gpui::{point, px};
        window.set_traffic_light_position(point(
            px(TRAFFIC_LIGHT_INSET),
            px((HEADER_HEIGHT - lights.size) / 2.),
        ));
    }
}

#[cfg(target_os = "macos")]
fn measure(window: &Window) -> Option<Lights> {
    use objc2_app_kit::{NSView, NSWindowButton};
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};

    let RawWindowHandle::AppKit(handle) = HasWindowHandle::window_handle(window).ok()?.as_raw()
    else {
        return None;
    };
    // SAFETY: gpui hands out its window's live content view, and this runs on
    // the main thread inside a gpui callback, where that view is alive.
    let view: &NSView = unsafe { handle.ns_view.cast().as_ref() };
    let window = view.window()?;
    let close = window
        .standardWindowButton(NSWindowButton::CloseButton)?
        .frame();
    let minimize = window
        .standardWindowButton(NSWindowButton::MiniaturizeButton)?
        .frame();
    Some(Lights {
        size: close.size.width as f32,
        pitch: (minimize.origin.x - close.origin.x) as f32,
    })
}

#[cfg(not(target_os = "macos"))]
fn measure(_: &Window) -> Option<Lights> {
    None
}
