//! The window's own buttons — minimize, maximize, close — where the platform
//! draws none.
//!
//! Every window here is opened with `appears_transparent` on its titlebar,
//! which on macOS keeps the traffic lights and drops the strip they sat in.
//! On Windows the same option drops the whole non-client frame, caption
//! buttons included: gpui hit-tests what the app draws instead, and a div
//! marked [`WindowControlArea::Close`] *is* the close button — Windows
//! handles the press, and a strip marked `Drag` is the caption, with the
//! double-click and the snap gestures that come with one. So the three are
//! drawn here, at the size and in the corner Windows puts its own, and only
//! on Windows: on macOS AppKit's are already on screen, and Linux keeps its
//! server-side decorations unless asked otherwise.
//!
//! [`WindowControlArea::Close`]: bezel::gpui::WindowControlArea::Close

use bezel::gpui::{AnyElement, App, Window, prelude::*};

/// The caption buttons, for the top-right corner of a window's root — or
/// nothing, on a platform that draws its own.
pub(crate) fn controls(window: &Window, cx: &App) -> Option<AnyElement> {
    #[cfg(target_os = "windows")]
    {
        Some(buttons::row(window, cx))
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = (window, cx);
        None
    }
}

/// Mark `el` as somewhere the window can be taken hold of, on the platform
/// whose frame this module stands in for. Elsewhere it is left alone: macOS
/// has its own titlebar drag, and on Linux the mark would ask the compositor
/// for a move the server-side frame already offers.
pub(crate) fn draggable<E: InteractiveElement>(el: E) -> E {
    #[cfg(target_os = "windows")]
    {
        el.window_control_area(bezel::gpui::WindowControlArea::Drag)
    }
    #[cfg(not(target_os = "windows"))]
    {
        el
    }
}

#[cfg(target_os = "windows")]
mod buttons {
    use crate::view::root::HEADER_HEIGHT;
    use bezel::{
        gpui::{AnyElement, App, Div, Hsla, Window, WindowControlArea, div, prelude::*, px, white},
        theme::Theme,
        ui::icons,
    };

    /// Each button's width. Windows 11 draws its own at 46 logical pixels;
    /// the height is the header's, so the row reads as the strip it replaces.
    const BUTTON_WIDTH: f32 = 46.;
    /// The glyphs are ten pixels square, as the system's own are.
    const GLYPH: f32 = 10.;

    pub(super) fn row(window: &Window, cx: &App) -> AnyElement {
        let theme = Theme::of(cx);
        let tint = theme.text;
        let zoom = if window.is_maximized() {
            restore(tint)
        } else {
            maximize(tint)
        };
        div()
            .absolute()
            .top_0()
            .right_0()
            .h(px(HEADER_HEIGHT))
            .flex()
            .flex_row()
            .child(
                button(
                    "caption-minimize",
                    WindowControlArea::Min,
                    theme.element_hover,
                )
                .child(minimize(tint)),
            )
            .child(
                button(
                    "caption-maximize",
                    WindowControlArea::Max,
                    theme.element_hover,
                )
                .child(zoom),
            )
            // The system's own close goes red under the pointer, and the glyph
            // goes white to stay legible on it — white in both appearances,
            // which is why this one is not the text tint.
            .child(
                button(
                    "caption-close",
                    WindowControlArea::Close,
                    theme.danger_strong,
                )
                .child(
                    icons::icon(icons::system::CLOSE)
                        .size(px(GLYPH + 4.))
                        .text_color(tint)
                        .group_hover("caption-close", |glyph| glyph.text_color(white())),
                ),
            )
            .into_any_element()
    }

    /// One button: a hover wash, and the mark that makes Windows treat it as
    /// the control it stands for. Nothing is wired to a click — the press is
    /// the platform's, delivered as `WM_NCLBUTTONUP` on the area it hit.
    fn button(group: &'static str, area: WindowControlArea, hover: Hsla) -> Div {
        div()
            .group(group)
            .w(px(BUTTON_WIDTH))
            .h_full()
            .flex()
            .items_center()
            .justify_center()
            .hover(move |el| el.bg(hover))
            .window_control_area(area)
    }

    /// A horizontal rule.
    fn minimize(tint: Hsla) -> Div {
        div().w(px(GLYPH)).h(px(1.)).bg(tint)
    }

    /// An outlined square.
    fn maximize(tint: Hsla) -> Div {
        div()
            .size(px(GLYPH))
            .border_1()
            .border_color(tint)
            .rounded(px(1.))
    }

    /// Two squares offset, the front one over the back — the system's own
    /// glyph for a window that is maximized and can be put back.
    fn restore(tint: Hsla) -> Div {
        let square = || {
            div()
                .absolute()
                .size(px(GLYPH - 2.))
                .border_1()
                .border_color(tint)
                .rounded(px(1.))
        };
        div()
            .relative()
            .size(px(GLYPH))
            .child(square().top_0().right_0())
            .child(square().bottom_0().left_0())
    }
}
