//! The "+ Add …" row a list ends with.

use bezel::{
    gpui::{Div, ElementId, Hsla, Stateful, div, prelude::*, px},
    theme::{TextStyle, Theme, Typeset},
    ui::{icons, widgets::Buttons},
};

/// A ghost row holding a plus and `label`, the label in `tint`. Its padding
/// and what a press does are the caller's.
pub(crate) fn row(
    id: impl Into<ElementId>,
    label: &'static str,
    tint: Hsla,
    theme: &Theme,
) -> Stateful<Div> {
    theme
        .ghost(id)
        .gap(px(6.))
        .child(
            icons::icon(icons::math::Plus)
                .size(px(12.))
                .text_color(theme.text_faint),
        )
        .child(
            div()
                .text_style(TextStyle::Callout)
                .text_color(tint)
                .child(label),
        )
}
