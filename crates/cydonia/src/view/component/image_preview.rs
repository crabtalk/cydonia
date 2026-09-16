//! Shared image-preview frame and controls.
use bezel::{
    gpui::{self, div, prelude::*, px},
    theme::Theme,
    ui::{icons, surface, tooltip::Tooltip},
};

pub(super) fn disc(
    theme: &Theme,
    id: impl Into<gpui::ElementId>,
    side: f32,
) -> gpui::Stateful<gpui::Div> {
    div()
        .id(id)
        .size(px(side))
        .rounded_full()
        .flex()
        .items_center()
        .justify_center()
        .bg(theme.solid)
        .cursor_pointer()
        .hover(|button| button.opacity(0.85))
        .child(
            icons::icon(icons::notifications::X)
                .size(px(side * 0.6))
                .text_color(theme.on_solid),
        )
}

pub(super) fn frame(
    theme: &Theme,
    close_id: &'static str,
    content: impl IntoElement,
    on_close: impl Fn(&gpui::ClickEvent, &mut gpui::Window, &mut gpui::App) + 'static,
) -> gpui::Div {
    div()
        .relative()
        .child(content)
        // Images paint in their own layer; keep the close control above them.
        .child(surface::layered(
            disc(theme, close_id, 24.)
                .debug_selector(move || close_id.into())
                .absolute()
                .top(px(10.))
                .right(px(10.))
                .tooltip(|window, cx| Tooltip::text("Close", window, cx))
                .on_click(on_close),
        ))
}
