//! Saved base sizes. Article and terminal keyboard zoom stays separate, so
//! resetting zoom always has a configured size to return to.

use crate::{
    model::settings as config,
    view::settings::{self, SettingsWindow},
};
use bezel::{
    gpui::{AnyElement, Context, div, prelude::*, px},
    theme::{TextStyle, Theme, Typeset},
    ui::widgets::{Buttons, Scaffolding},
};

#[derive(Clone, Copy)]
enum Font {
    Ui,
    Article,
    Terminal,
}

impl Font {
    fn key(self) -> &'static str {
        match self {
            Self::Ui => "ui-font",
            Self::Article => "article-font",
            Self::Terminal => "terminal-font",
        }
    }

    fn title(self) -> &'static str {
        match self {
            Self::Ui => "UI font size",
            Self::Article => "Article font size",
            Self::Terminal => "Terminal font size",
        }
    }

    fn description(self) -> &'static str {
        match self {
            Self::Ui => "Sizes for menus, controls, and the rest of the interface.",
            Self::Article => "Default for articles. ⌘+/− zooms; ⌘0 resets.",
            Self::Terminal => "Default for terminals. ⌘+/− zooms; ⌘0 resets.",
        }
    }

    fn range(self) -> (f32, f32) {
        match self {
            Self::Ui => config::TEXT_SIZE,
            _ => config::CONTENT_TEXT_SIZE,
        }
    }
}

impl SettingsWindow {
    pub(super) fn typography_group(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = Theme::of(cx).clone();
        let workspace = self.workspace.read(cx);
        let sizes = [
            workspace.text_size,
            workspace.article_font_size(),
            workspace.terminal_font_size,
        ];
        div()
            .flex()
            .flex_col()
            .gap(px(settings::LABEL_GAP))
            .child(theme.field_label("Typography"))
            .child(
                theme.group_box().children(
                    [Font::Ui, Font::Article, Font::Terminal]
                        .into_iter()
                        .zip(sizes)
                        .enumerate()
                        .map(|(ix, (font, size))| {
                            theme
                                .card_row(ix == 0)
                                .child(
                                    div()
                                        .flex_1()
                                        .min_w_0()
                                        .flex()
                                        .flex_col()
                                        .child(theme.row_title(font.title()))
                                        .child(
                                            div()
                                                .mt(px(4.))
                                                .text_style(TextStyle::Subheadline)
                                                .text_color(theme.text_muted)
                                                .child(font.description()),
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
                                        .child(self.font_step(font, size, -1., cx))
                                        .child(
                                            div()
                                                .w(px(34.))
                                                .flex()
                                                .justify_center()
                                                .text_style(TextStyle::Callout)
                                                .text_color(theme.text)
                                                .child(format!("{size:.0}")),
                                        )
                                        .child(self.font_step(font, size, 1., cx)),
                                )
                        }),
                ),
            )
            .into_any_element()
    }

    fn font_step(&self, font: Font, size: f32, by: f32, cx: &mut Context<Self>) -> AnyElement {
        let theme = Theme::of(cx).clone();
        let (min, max) = font.range();
        let next = (size + by).clamp(min, max);
        theme
            .ghost((font.key(), usize::from(by > 0.)))
            .px(px(8.))
            .py(px(3.))
            .text_style(TextStyle::Callout)
            .text_color(if next == size {
                theme.text_faint
            } else {
                theme.text
            })
            .child(if by < 0. { "−" } else { "+" })
            .on_click(cx.listener(move |this, _, _, cx| {
                if next == size {
                    return;
                }
                this.workspace.update(cx, |workspace, cx| match font {
                    Font::Ui => workspace.set_text_size(next, cx),
                    Font::Article => workspace.set_article_font_size(next, cx),
                    Font::Terminal => workspace.set_terminal_font_size(next, cx),
                });
                cx.notify();
            }))
            .into_any_element()
    }
}
