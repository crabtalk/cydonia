//! Labelled fields, as the board dialogs ask for a name and a key and the
//! agent modal for an identity's.

use bezel::{
    gpui::{AnyElement, App, AppContext as _, Div, Entity, SharedString, div, prelude::*, px},
    theme::{TextStyle, Theme, Typeset},
    ui::input::TextField,
};

/// How wide a field's label runs, so the fields in one form start on one edge.
const LABEL_WIDTH: f32 = 34.;

/// A field claiming `context`, holding `content`.
pub(crate) fn field(
    context: &'static str,
    content: impl Into<SharedString>,
    placeholder: &'static str,
    cx: &mut App,
) -> Entity<TextField> {
    let field = cx.new(|cx| {
        TextField::new(cx)
            .with_key_context(context)
            .with_placeholder(placeholder)
    });
    field.update(cx, |field, cx| field.set_content(content.into(), cx));
    field
}

/// `field` with `label` before it.
pub(crate) fn row(theme: &Theme, label: &str, field: Entity<TextField>) -> AnyElement {
    labelled(theme, LABEL_WIDTH, label, field)
}

/// `field` with `label` before it in a column `width` across.
pub(crate) fn labelled(
    theme: &Theme,
    width: f32,
    label: &str,
    field: impl IntoElement,
) -> AnyElement {
    div()
        .flex()
        .flex_row()
        .items_center()
        .gap(px(8.))
        .child(
            div()
                .flex_none()
                .w(px(width))
                .text_style(TextStyle::Caption)
                .text_color(theme.text_muted)
                .child(label.to_owned()),
        )
        .child(div().flex_1().min_w_0().child(field))
        .into_any_element()
}

/// Why the form was refused.
pub(crate) fn error(theme: &Theme, why: SharedString) -> Div {
    div()
        .text_style(TextStyle::Caption)
        .text_color(theme.danger)
        .child(why)
}
