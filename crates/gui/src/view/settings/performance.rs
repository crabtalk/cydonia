//! The performance section: the switch that puts the frame meter on every
//! window, and what the app is holding while you use it.
//!
//! The switch is here, the meter is not: it lives on the windows themselves,
//! so closing this one leaves the app's own meter where you dragged it. The
//! counts under it are the other half of the same question — a rate belongs on
//! an instrument you can watch, an inventory belongs on a page you open.

use crate::{
    model::{watch, workspace::Resident},
    view::settings::{self, SettingsWindow, Switch, group, labelled},
};
use bezel::{
    gpui::{AnyElement, Context, div, prelude::*, px},
    theme::{TextStyle, Theme, Typeset},
    ui::widgets::{Controls, Scaffolding},
};

/// The watch delays the row offers, named for what they buy rather than for
/// what they are. The middle one is [`watch::BOUNCE`], which is what a file
/// that has never been touched carries.
const BOUNCES: [(u64, &str); 3] = [
    (50, "Instant"),
    (watch::BOUNCE, "Balanced"),
    (500, "Relaxed"),
];

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
                    .child(self.watch_bounce_row(cx)),
            )
            .child(self.resident_group(cx))
            .into_any_element()
    }

    fn meter_row(&self, cx: &mut Context<Self>) -> AnyElement {
        let on = self.workspace.read(cx).meter;
        self.switch_row(
            Switch::new(
                "meter",
                "Frame meter",
                "What this window draws while you use it.",
                on,
            )
            .first(true),
            cx,
            |this, cx| {
                this.workspace.update(cx, |workspace, cx| {
                    workspace.meter = !workspace.meter;
                    cx.notify();
                });
            },
        )
    }

    /// How long a project's `.cydonia/` has to go quiet before the watch
    /// re-reads it — see [`crate::model::watch`].
    ///
    /// A segmented control rather than a field: the useful values are three,
    /// and what they are worth saying is what they cost, not what they are.
    /// `settings.toml` still takes any number in the range, and a file carrying
    /// one lights no segment — the note underneath is what says where it sits.
    fn watch_bounce_row(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let theme = Theme::of(cx).clone();
        // What the watch will actually wait, not what the file says: the ends
        // of the range are enforced on the way out of it, and a row reporting
        // the number it was given would be reporting one nothing honours.
        let ms = watch::bounce(self.workspace.read(cx).settings.watch_bounce).as_millis();
        theme
            .card_row(false)
            .child(labelled(
                &theme,
                "Watch delay",
                format!("Changes on disk show after {ms} ms of quiet."),
                false,
            ))
            .child(
                theme
                    .segmented(
                        "watch-bounce",
                        BOUNCES.map(|(_, label)| label),
                        BOUNCES
                            .iter()
                            .position(|(value, _)| u128::from(*value) == ms)
                            .unwrap_or(usize::MAX),
                        cx.listener(|this, ix: &usize, _, cx| {
                            this.workspace.update(cx, |workspace, cx| {
                                workspace.set_watch_bounce(BOUNCES[*ix].0, cx)
                            });
                            cx.notify();
                        }),
                    )
                    .flex_none(),
            )
    }

    /// What is in memory, counted when the page is drawn. Read-only: this is
    /// the app answering for itself, not another thing to configure.
    fn resident_group(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = Theme::of(cx).clone();
        let Resident {
            projects,
            articles,
            documents,
            sessions,
            items,
        } = self.workspace.read(cx).resident(cx);
        group(&theme, "Resident")
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
                        "Holding a document, of those listed.",
                        format!("{documents} of {articles}"),
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
            .child(labelled(&theme, title, note, false))
            .child(
                div()
                    .flex_none()
                    .text_style(TextStyle::Callout)
                    .text_color(theme.text)
                    .child(value),
            )
    }
}
