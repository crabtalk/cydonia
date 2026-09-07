//! The features section: the surfaces a project can hold, and which of them
//! this install shows.
//!
//! Everything here is off in a fresh `settings.toml`, so what the app opens as
//! is articles and nothing else. The switches are the room's whole content —
//! what each one costs or unlocks is said on its own row, because the reasons
//! are not the same: sessions run a program on this machine, and boards and
//! tables are finished work held back to keep the first release one thing.

use crate::{model::settings::Feature, view::settings::SettingsWindow};
use bezel::{
    gpui::{AnyElement, Context, div, prelude::*, px},
    theme::{TextStyle, Theme, Typeset},
    ui::{
        icons,
        widgets::{Content, Controls, Scaffolding},
    },
};

/// What a row says for itself: its mark, its name, what turning it on means,
/// and — where maturity rather than scope is the reason it is off — a badge.
///
/// Beta is a claim about a row, not about the room. Sessions carry none: an
/// agent that downloads and runs a package is not immature, it is something to
/// be asked for.
fn copy(
    feature: Feature,
) -> (
    &'static str,
    &'static str,
    &'static str,
    Option<&'static str>,
) {
    match feature {
        Feature::Sessions => (
            icons::system::CHAT_ROUND_LINE,
            "Sessions",
            "Runs an agent — a package this machine downloads and executes.",
            None,
        ),
        Feature::Boards => (
            icons::editing::LIST,
            "Boards",
            "Cards in columns, one board to a file.",
            Some("Preview"),
        ),
        Feature::Tables => (
            icons::system::WIDGET,
            "Tables",
            "Structured records in the project's store.",
            Some("Preview"),
        ),
    }
}

impl SettingsWindow {
    pub(super) fn features_body(&self, cx: &Context<Self>) -> AnyElement {
        let theme = Theme::of(cx).clone();
        // The line under the title is the section's, but it is rendered with
        // the header — see `Section::subtitle`.
        theme
            .group_box()
            .children(
                Feature::ALL
                    .into_iter()
                    .enumerate()
                    .map(|(ix, feature)| self.feature_row(ix, feature, cx)),
            )
            .into_any_element()
    }

    fn feature_row(&self, ix: usize, feature: Feature, cx: &Context<Self>) -> AnyElement {
        let theme = Theme::of(cx).clone();
        let (glyph, title, blurb, badge) = copy(feature);
        let on = feature.on(&self.workspace.read(cx).settings.features);
        theme
            .card_row(ix == 0)
            .child(
                div()
                    .flex_none()
                    .size(px(18.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(
                        icons::icon(glyph)
                            .size(px(16.))
                            .flex_none()
                            .text_color(theme.text_muted),
                    ),
            )
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
                            // One line, whatever the window is doing: a row
                            // that grows a second one moves every switch below
                            // it down the column.
                            .truncate()
                            .text_style(TextStyle::Subheadline)
                            .text_color(theme.text_muted)
                            .child(blurb),
                    ),
            )
            .children(badge.map(|label| theme.badge(label)))
            .child(
                div()
                    .id(("feature", ix))
                    .cursor_pointer()
                    .child(theme.toggle(on))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.workspace
                            .update(cx, |workspace, cx| workspace.set_feature(feature, !on, cx));
                        cx.notify();
                    })),
            )
            .into_any_element()
    }
}
