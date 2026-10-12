//! A question asked over the whole window, and the row of buttons that answers
//! a dialog or a panel.

use bezel::{
    gpui::{AnyElement, App, ClickEvent, Div, MouseDownEvent, Window, div, prelude::*, px},
    theme::Theme,
    ui::{
        popover,
        widgets::{ButtonStyle, Buttons},
    },
};

/// What a button does when pressed.
pub(crate) type Press = Box<dyn Fn(&ClickEvent, &mut Window, &mut App)>;

/// `content` on bezel's dialog card over a scrim. A press on the scrim calls
/// `dismiss`.
pub(crate) fn dialog(
    id: &'static str,
    theme: &Theme,
    window: &Window,
    content: impl IntoIterator<Item = AnyElement>,
    dismiss: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
) -> AnyElement {
    let card = popover::dialog_card(theme).gap(px(10.)).children(content);
    popover::modal(id, window.viewport_size(), card.into_any_element(), dismiss)
}

/// Cancel, where there is one, then the prominent `answer`. Ids are `(id, 0)`
/// and `(id, 1)`.
pub(crate) fn answers(
    theme: &Theme,
    id: &'static str,
    cancel: Option<Press>,
    answer: &'static str,
    on_answer: Press,
) -> Div {
    div()
        .mt(px(4.))
        .flex()
        .flex_row()
        .justify_end()
        .gap(px(8.))
        .children(cancel.map(|cancel| {
            theme
                .button("Cancel", ButtonStyle::Ghost, None)
                .id((id, 0usize))
                .on_click(cancel)
        }))
        .child(
            theme
                .button(answer, ButtonStyle::Prominent, None)
                .id((id, 1usize))
                .on_click(on_answer),
        )
}
