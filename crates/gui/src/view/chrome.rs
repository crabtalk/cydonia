//! The window's own caption buttons and drag handle.
//!
//! Every window opens with `appears_transparent` and `app_owns_titlebar_drag`,
//! and on Linux asks for client decorations, so nothing but a [`grip`] moves
//! it. Off macOS the caption buttons are the only ones it has. Each lands in
//! whichever band sits at that corner of the window, the way
//! [`crate::view::root::toolbar_inset`] is taken by whichever sits under the
//! traffic lights.
//!
//! On macOS AppKit paints the lights. In a browser both are inert: the page
//! owns the frame the window sits in.

use bezel::{
    gpui::{AnyElement, App, ElementId, Window, WindowDecorations, div, prelude::*},
    ui::{
        AppExt as _,
        titlebar::{self, CaptionSide, DragState},
    },
};

const NATIVE: bool = cfg!(any(target_os = "macos", target_family = "wasm"));
const WEB: bool = cfg!(target_family = "wasm");

/// The caption buttons for `side`, or nothing where [`has`] says none.
pub fn caption(side: CaptionSide, window: &Window, cx: &App) -> Option<AnyElement> {
    has(side, window, cx).then(|| {
        titlebar::controls(side, window, cx)
            .flex_none()
            .into_any_element()
    })
}

/// The free stretch of a band. It drags the window, zooms it on a double
/// click and, off macOS, opens the window menu on a right press.
pub fn grip(id: impl Into<ElementId>, drag: &DragState, window: &Window) -> AnyElement {
    match WEB {
        true => div().flex_1().min_w_0().into_any_element(),
        false => titlebar::grip(id, drag, window)
            .min_w_0()
            .into_any_element(),
    }
}

/// Whether [`caption`] draws buttons on `side`: never on macOS, in a browser
/// or in full screen. Traffic lights are always on the left; the rectangular
/// buttons go on the side the desktop's layout names, or all on the right
/// where there is no layout. A band that has them
/// drops its inset on that side, so the buttons sit flush with the window's
/// edge.
pub fn has(side: CaptionSide, window: &Window, cx: &App) -> bool {
    if NATIVE || window.is_fullscreen() {
        return false;
    }
    let layout = Some(
        cx.caption_style()
            .button_layout(cx.button_layout(), window.window_controls()),
    );
    match side {
        CaptionSide::Left => layout.is_some_and(|layout| layout.left[0].is_some()),
        CaptionSide::Right => layout.is_none_or(|layout| layout.right[0].is_some()),
    }
}

/// What a window asks the desktop for. Client on Linux, so the frame and the
/// buttons are the same under every compositor; ignored elsewhere.
pub fn decorations() -> Option<WindowDecorations> {
    cfg!(any(target_os = "linux", target_os = "freebsd")).then_some(WindowDecorations::Client)
}
