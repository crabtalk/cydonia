//! The features section: the surfaces a project can hold, and which of them
//! this install shows.
//!
//! Everything here is off in a fresh `settings.toml`, so what the app opens as
//! is articles and nothing else. The switches are the room's whole content —
//! what each one costs or unlocks is said on its own row, because the reasons
//! are not the same: sessions run a program on this machine, and boards and
//! tables are finished work held back to keep the first release one thing.

use crate::{
    model::settings::Feature,
    view::settings::{SettingsWindow, Section},
};
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
fn copy(feature: Feature) -> (&'static str, &'static str, &'static str, Option<&'static str>) {
    match feature {
        Feature::Sessions => (
            icons::CHAT_ROUND_LINE,
            "Sessions",
            "A session runs an agent — a package this machine downloads and \
             executes. None starts while this is off.",
            None,
        ),
        Feature::Boards => (
            icons::LIST,
            "Boards",
            "Cards in columns, one board to a file. Boards already written stay \
             in the project while this is off.",
            Some("Preview"),
        ),
        Feature::Tables => (
            icons::WIDGET,
            "Tables",
            "Structured records in the project's store. The store is left alone \
             while this is off.",
            Some("Preview"),
        ),
    }
}

impl SettingsWindow {
    pub(super) fn features_body(&self, cx: &Context<Self>) -> AnyElement {
        let theme = Theme::of(cx).clone();
        div()
            .flex()
            .flex_col()
            .child(theme.page_subtitle(
                "Parts of cydonia that stay off until you ask for them. Turning one \
                 off hides it; nothing on disk is deleted.",
            ))
            .child(
                theme.group_box().mt(px(super::GROUP_GAP)).children(
                    Feature::ALL
                        .into_iter()
                        .enumerate()
                        .map(|(ix, feature)| self.feature_row(ix, feature, cx)),
                ),
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

    /// What the agents section shows in place of the gate that used to live
    /// there: where the switch went, and one press to get to it. A second
    /// toggle on the same flag would be two pieces of copy to keep in step.
    pub(super) fn sessions_off(&self, cx: &Context<Self>) -> Option<AnyElement> {
        let theme = Theme::of(cx).clone();
        if self.workspace.read(cx).settings.features.sessions {
            return None;
        }
        Some(
            theme
                .group_box()
                .child(
                    theme
                        .card_row(true)
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .text_style(TextStyle::Subheadline)
                                .text_color(theme.text_muted)
                                .child(
                                    "Sessions are off, so nothing installed here can be \
                                     launched. Agents can still be installed and removed.",
                                ),
                        )
                        .child(
                            div()
                                .id("to-features")
                                .flex_none()
                                .cursor_pointer()
                                .text_style(TextStyle::Callout)
                                .text_color(theme.accent)
                                .child("Features")
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.show(Section::Features, cx)
                                })),
                        ),
                )
                .into_any_element(),
        )
    }
}
