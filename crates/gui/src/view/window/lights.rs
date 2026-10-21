//! The macOS traffic lights' frame and spacing, read off a window's own
//! buttons. They differ across macOS releases and AppKit owns them, so the
//! header asks rather than assumes. A button's frame need not be square:
//! gpui places it by its full height.

use crate::view::window::root::{HEADER_HEIGHT, TRAFFIC_LIGHT_INSET};
use bezel::gpui::{App, Global, Window};

/// A light's size on macOS 26.
pub(crate) const LIGHT_SIZE: f32 = 14.;

/// What macOS 26 draws, for a window not measured yet and off macOS.
const MACOS_26: Lights = Lights {
    size: LIGHT_SIZE,
    height: LIGHT_SIZE,
    pitch: 23.,
};

/// One light's frame and the distance between two lights' left edges.
#[derive(Clone, Copy)]
pub struct Lights {
    pub size: f32,
    pub height: f32,
    pub pitch: f32,
}

impl Global for Lights {}

impl Lights {
    /// The lights measured so far, or macOS 26's.
    pub fn of(cx: &App) -> Self {
        cx.try_global::<Self>().copied().unwrap_or(MACOS_26)
    }

    /// How far the close button stands in from the window's left edge under a
    /// top band `band` tall: [`TRAFFIC_LIGHT_INSET`] under the header's, and
    /// further in by as much as a taller band centres the lights further down.
    pub fn left(band: f32) -> f32 {
        TRAFFIC_LIGHT_INSET + (band - HEADER_HEIGHT) / 2.
    }

    /// Where the green light's right edge lands under a top band `band` tall.
    pub fn end(&self, band: f32) -> f32 {
        Self::left(band) + 2. * self.pitch + self.size
    }
}

/// Measure `window`'s lights, record them for every band to read, and centre
/// the lights down a top band `band` tall, as far in from the left edge as
/// [`Lights::left`] says. Call once the window exists, and again when its band
/// changes height.
pub fn fit(window: &mut Window, band: f32, cx: &mut App) {
    let lights = measure(window).unwrap_or(MACOS_26);
    cx.set_global(lights);
    // gpui has the setter on macOS only.
    #[cfg(target_os = "macos")]
    {
        use bezel::gpui::{point, px};
        window.set_traffic_light_position(point(
            px(Lights::left(band)),
            px((band - lights.height) / 2.),
        ));
    }
    #[cfg(not(target_os = "macos"))]
    let _ = band;
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
        height: close.size.height as f32,
        pitch: (minimize.origin.x - close.origin.x) as f32,
    })
}

#[cfg(not(target_os = "macos"))]
fn measure(_: &Window) -> Option<Lights> {
    None
}
