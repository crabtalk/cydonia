//! The performance section: the switch that puts the frame meter on every
//! window, and what the app is holding while you use it.
//!
//! The switch is here, the meter is not: it lives on the windows themselves,
//! so closing this one leaves the app's own meter where you dragged it. The
//! counts under it are the other half of the same question — a rate belongs on
//! an instrument you can watch, an inventory belongs on a page you open.

use crate::{
    model::{cover, workspace::Resident},
    view::settings::{self, SettingsWindow},
};
use bezel::{
    gpui::{AnyElement, Context, Focusable as _, Window, div, prelude::*, px},
    theme::{TextStyle, Theme, Typeset},
    ui::{
        input::{Shape, TextField},
        widgets::{ButtonStyle, Buttons, Controls, Scaffolding},
    },
};

/// The least the ceiling may be set to: one cover, rounded up to whole
/// megabytes. Below it the open article would be evicted as it is drawn.
const FLOOR: u64 = cover::RASTER_BYTES / 1_000_000 + 1;

/// How wide the ceiling's dialog sits.
const DIALOG_WIDTH: f32 = 320.;

/// How many covers a ceiling of `mb` admits — the capacity the cache runs at,
/// which is the ceiling floored to whole pictures.
fn covers_under(mb: u64) -> String {
    match mb * 1_000_000 / cover::RASTER_BYTES {
        1 => "one cover".to_owned(),
        n => format!("{n} covers"),
    }
}

impl SettingsWindow {
    pub(super) fn performance_body(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = Theme::of(cx).clone();
        div()
            .flex()
            .flex_col()
            .gap(px(settings::GROUP_GAP))
            .child(
                theme
                    .group_box()
                    .child(self.meter_row(cx))
                    .child(self.cover_memory_row(cx)),
            )
            .child(self.resident_group(cx))
            .into_any_element()
    }

    fn meter_row(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let theme = Theme::of(cx).clone();
        let on = self.workspace.read(cx).meter;
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
                            .child("What this window draws while you use it."),
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
            )
    }

    /// The ceiling decoded covers run under. Typed rather than stepped: the
    /// useful values are far apart, and a press each way reaches none of them.
    fn cover_memory_row(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let theme = Theme::of(cx).clone();
        let mb = self.workspace.read(cx).settings.cover_memory;
        theme
            .card_row(false)
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .child(theme.row_title("Cover memory"))
                    .child(
                        div()
                            .mt(px(4.))
                            .text_style(TextStyle::Subheadline)
                            .text_color(theme.text_muted)
                            .child(format!("Room for {}.", covers_under(mb))),
                    ),
            )
            .child(
                theme
                    .ghost("cover-memory")
                    .px(px(10.))
                    .py(px(3.))
                    .border_1()
                    .border_color(theme.border)
                    .bg(theme.input_bg)
                    .text_style(TextStyle::Callout)
                    .text_color(theme.text)
                    .child(format!("{mb} MB"))
                    .on_click(
                        cx.listener(|this, _, window, cx| this.edit_cover_memory(window, cx)),
                    ),
            )
    }

    /// Put the ceiling in a field, seeded with what it is now.
    fn edit_cover_memory(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let mb = self.workspace.read(cx).settings.cover_memory;
        let field = cx.new(|cx| {
            let mut field = TextField::new(cx).with_shape(Shape::Line);
            field.set_content(mb.to_string(), cx);
            field
        });
        field.focus_handle(cx).focus(window, cx);
        self.editing = Some(field);
        cx.notify();
    }

    /// Take what was typed, if it is a number. Anything else leaves the
    /// ceiling where it was rather than guessing at what was meant.
    fn save_cover_memory(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(field) = self.editing.take() else {
            return;
        };
        if let Ok(mb) = field.read(cx).content().trim().parse::<u64>() {
            let mb = mb.max(FLOOR);
            self.workspace.update(cx, |workspace, cx| {
                workspace.set_cover_memory(mb, window, cx)
            });
        }
        cx.notify();
    }

    /// The ceiling's dialog, over a scrim that takes the press that dismisses
    /// it. Rendered by the window, so it sits above the scrolling body.
    pub(super) fn cover_dialog(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let field = self.editing.clone()?;
        let theme = Theme::of(cx).clone();
        Some(
            div()
                .id("cover-scrim")
                .absolute()
                .inset_0()
                .flex()
                .items_center()
                .justify_center()
                .bg(theme.scrim())
                .on_click(cx.listener(|this, _, _, cx| {
                    this.editing = None;
                    cx.notify();
                }))
                .child(
                    div()
                        .id("cover-dialog")
                        .w(px(DIALOG_WIDTH))
                        .flex()
                        .flex_col()
                        .gap(px(settings::LABEL_GAP))
                        .p(px(20.))
                        .rounded(px(Theme::panel_radius()))
                        .border_1()
                        .border_color(theme.border)
                        .bg(theme.surface)
                        // The press that opens a field must not reach the scrim.
                        .on_click(|_, _, cx| cx.stop_propagation())
                        .child(theme.row_title("Cover memory"))
                        .child(
                            div()
                                .text_style(TextStyle::Subheadline)
                                .text_color(theme.text_muted)
                                .child("In megabytes."),
                        )
                        .child(field)
                        .child(
                            div()
                                .flex()
                                .flex_row()
                                .justify_end()
                                .gap(px(8.))
                                .child(
                                    theme
                                        .button("Cancel", ButtonStyle::Ghost, None)
                                        .id("cover-cancel")
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            this.editing = None;
                                            cx.notify();
                                        })),
                                )
                                .child(
                                    theme
                                        .button("Save", ButtonStyle::Prominent, None)
                                        .id("cover-save")
                                        .on_click(cx.listener(|this, _, window, cx| {
                                            this.save_cover_memory(window, cx)
                                        })),
                                ),
                        ),
                )
                .into_any_element(),
        )
    }

    /// What is in memory, counted when the page is drawn. Read-only: this is
    /// the app answering for itself, not another thing to configure.
    fn resident_group(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = Theme::of(cx).clone();
        let Resident {
            projects,
            articles,
            editors,
            covers,
            sessions,
            items,
        } = self.workspace.read(cx).resident(cx);
        div()
            .flex()
            .flex_col()
            .gap(px(settings::LABEL_GAP))
            .child(theme.field_label("Resident"))
            .child(
                theme
                    .group_box()
                    .child(self.stat_row(
                        true,
                        "Projects",
                        "Open on the rail.",
                        projects.to_string(),
                        cx,
                    ))
                    .child(self.stat_row(
                        false,
                        "Articles",
                        "Holding an editor, of those listed.",
                        format!("{editors} of {articles}"),
                        cx,
                    ))
                    .child(self.stat_row(
                        false,
                        "Covers",
                        "Decoded and held, at twice their declared size.",
                        format!("{covers} · {} MB each", cover::RASTER_BYTES / 1_000_000),
                        cx,
                    ))
                    .child(self.stat_row(
                        false,
                        "Transcripts",
                        "Sessions in memory, and the entries across them.",
                        format!("{sessions} · {items} entries"),
                        cx,
                    )),
            )
            .into_any_element()
    }

    fn stat_row(
        &self,
        first: bool,
        title: &'static str,
        note: &'static str,
        value: String,
        cx: &Context<Self>,
    ) -> impl IntoElement + use<> {
        let theme = Theme::of(cx).clone();
        theme
            .card_row(first)
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .child(theme.row_title(title))
                    .child(
                        div()
                            .mt(px(4.))
                            .text_style(TextStyle::Subheadline)
                            .text_color(theme.text_muted)
                            .child(note),
                    ),
            )
            .child(
                div()
                    .flex_none()
                    .text_style(TextStyle::Callout)
                    .text_color(theme.text)
                    .child(value),
            )
    }
}
