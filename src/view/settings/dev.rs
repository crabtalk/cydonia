//! The dev section: the switch that puts the frame meter on every window.
//!
//! The switch is here, the meter is not: it lives on the windows themselves,
//! so closing this one leaves the app's own meter where you dragged it.

use crate::view::settings::SettingsWindow;
use bezel::{
    gpui::{AnyElement, Context, div, prelude::*, px},
    theme::{TextStyle, Theme, Typeset},
    ui::widgets::{Controls, Scaffolding},
};

impl SettingsWindow {
    pub(super) fn dev_body(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = Theme::of(cx).clone();
        let on = self.workspace.read(cx).meter;
        theme
            .group_box()
            .child(
                theme
                    .card_row(true)
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .child(theme.row_title("Frame meter"))
                            .child(
                                div()
                                    .mt(px(4.))
                                    .text_style(TextStyle::Subheadline)
                                    .text_color(theme.text_muted)
                                    .child(
                                        "What this window draws, and what it costs, \
                                         while you use it.",
                                    ),
                            ),
                    )
                    .child(
                        div()
                            .id("meter")
                            .cursor_pointer()
                            .child(theme.toggle(on))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.workspace.update(cx, |workspace, cx| {
                                    workspace.meter = !workspace.meter;
                                    cx.notify();
                                });
                                cx.notify();
                            })),
                    ),
            )
            .into_any_element()
    }
}
