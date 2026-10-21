//! The dots under a run of pages or images: one per page, the one shown
//! filled, a press on one going to it.

use bezel::{
    gpui::{Div, SharedString, Stateful, div, prelude::*, px},
    theme::Theme,
    ui::{tooltip::Tooltip, widgets},
};

/// The dot for page `index`, named `name` with its index and tipped `tip`.
pub(crate) fn dot(
    name: &'static str,
    index: usize,
    shown: bool,
    tip: impl Into<SharedString>,
    theme: &Theme,
) -> Stateful<Div> {
    let tip = tip.into();
    div()
        .id((name, index))
        .debug_selector(move || format!("{name}-{index}"))
        .size(px(24.))
        .flex()
        .items_center()
        .justify_center()
        .cursor_pointer()
        .tooltip(move |window, cx| Tooltip::text(tip.clone(), window, cx))
        .child(widgets::status_dot(match shown {
            true => theme.text,
            false => theme.text_faint.opacity(0.4),
        }))
}
