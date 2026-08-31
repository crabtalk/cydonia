//! The appearance section: which of the three modes the app paints in.

use crate::view::settings::SettingsWindow;
use bezel::{
    gpui::{Context, div, prelude::*, px},
    theme::{
        TextStyle, Theme, Typeset,
        appearance::{self, AppearanceMode},
    },
    ui::widgets::Scaffolding,
};

const MODES: [AppearanceMode; 3] = [
    AppearanceMode::System,
    AppearanceMode::Light,
    AppearanceMode::Dark,
];

impl SettingsWindow {
    /// One card row: what the setting is on the left, the control on the right.
    pub(super) fn theme_row(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let theme = Theme::of(cx).clone();
        let current = appearance::mode(cx);
        theme
            .card_row(true)
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .child(theme.row_title("Theme"))
                    .child(
                        div()
                            .mt(px(4.))
                            .text_style(TextStyle::Subheadline)
                            .text_color(theme.text_muted)
                            .child("Follow the system, or pick one."),
                    ),
            )
            .child(
                // A segmented control rather than a select: three options that
                // all fit are worth showing at once.
                div()
                    .flex_none()
                    .flex()
                    .flex_row()
                    .gap(px(2.))
                    .p(px(2.))
                    .rounded(px(Theme::button_radius()))
                    .border_1()
                    .border_color(theme.border)
                    .children(MODES.into_iter().enumerate().map(|(ix, mode)| {
                        let selected = mode == current;
                        div()
                            .id(("appearance", ix))
                            .px(px(10.))
                            .py(px(4.))
                            .rounded(px(Theme::control_radius()))
                            .text_style(TextStyle::Callout)
                            .cursor_pointer()
                            .when(selected, |el| {
                                el.bg(theme.element_active).text_color(theme.text)
                            })
                            .when(!selected, |el| {
                                el.text_color(theme.text_muted)
                                    .hover(|el| el.bg(theme.element_hover))
                            })
                            .child(mode.label())
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.workspace
                                    .update(cx, |workspace, cx| workspace.set_appearance(mode, cx));
                                cx.notify();
                            }))
                    })),
            )
    }
}
