//! The typography group of the appearance page: what size the interface reads
//! at.
//!
//! One setting, because one is what the ladder needs — bezel's eleven roles
//! each keep their measured ratio to the body size, so moving it moves the
//! whole scale.

use crate::{
    model::state,
    view::settings::{self, SettingsWindow},
};
use bezel::{
    gpui::{AnyElement, Context, ElementId, SharedString, div, prelude::*, px},
    theme::{TextStyle, Theme, Typeset},
    ui::widgets::{Buttons, Scaffolding},
};

impl SettingsWindow {
    pub(super) fn typography_group(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = Theme::of(cx).clone();
        let size = self.workspace.read(cx).text_size;
        div()
            .flex()
            .flex_col()
            .gap(px(settings::LABEL_GAP))
            .child(theme.field_label("Typography"))
            .child(
                theme.group_box().child(
                    theme
                        .card_row(true)
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .flex()
                                .flex_col()
                                .child(theme.row_title("UI font size"))
                                .child(
                                    div()
                                        .mt(px(4.))
                                        .text_style(TextStyle::Subheadline)
                                        .text_color(theme.text_muted)
                                        .child("Every other size is a ratio of this one."),
                                ),
                        )
                        .child(
                            div()
                                .flex_none()
                                .flex()
                                .flex_row()
                                .items_center()
                                .rounded(px(Theme::button_radius()))
                                .border_1()
                                .border_color(theme.border)
                                .bg(theme.input_bg)
                                .child(self.step(size, -1., cx))
                                .child(
                                    div()
                                        .w(px(34.))
                                        .flex()
                                        .justify_center()
                                        .text_style(TextStyle::Callout)
                                        .text_color(theme.text)
                                        .child(format!("{size:.0}")),
                                )
                                .child(self.step(size, 1., cx)),
                        ),
                ),
            )
            .into_any_element()
    }

    /// One end of the stepper, spent at the range's edge.
    fn step(&self, size: f32, by: f32, cx: &mut Context<Self>) -> AnyElement {
        let theme = Theme::of(cx).clone();
        let (id, glyph): (ElementId, SharedString) = if by < 0. {
            ("size-down".into(), "−".into())
        } else {
            ("size-up".into(), "+".into())
        };
        let next = (size + by).clamp(state::TEXT_SIZE.0, state::TEXT_SIZE.1);
        theme
            .ghost(id)
            .px(px(8.))
            .py(px(3.))
            .text_style(TextStyle::Callout)
            .text_color(if next == size {
                theme.text_faint
            } else {
                theme.text
            })
            .child(glyph)
            .on_click(cx.listener(move |this, _, _, cx| {
                this.workspace
                    .update(cx, |workspace, cx| workspace.set_text_size(next, cx));
                cx.notify();
            }))
            .into_any_element()
    }
}
