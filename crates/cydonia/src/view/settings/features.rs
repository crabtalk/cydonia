//! The features section: the surfaces a project can hold, and which of them
//! this install shows.
//!
//! Sessions and boards default to on; tables are opt-in.

use crate::{
    model::settings::Feature,
    view::settings::{SettingsWindow, Switch},
};
use bezel::{
    gpui::{AnyElement, Context, prelude::*},
    theme::Theme,
    ui::{icons, widgets::Scaffolding},
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
    &'static [u8],
    &'static str,
    &'static str,
    Option<&'static str>,
) {
    match feature {
        Feature::Sessions => (
            icons::social::MessageCircle,
            "Sessions",
            "Runs an agent — a package this machine downloads and executes.",
            None,
        ),
        Feature::Boards => (
            icons::development::SquareKanban,
            "Boards",
            "Cards in columns, one board to a file.",
            None,
        ),
        Feature::Tables => (
            icons::files::Table2,
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
        let (glyph, title, blurb, badge) = copy(feature);
        let on = feature.on(&self.workspace.read(cx).settings.features);
        self.switch_row(
            Switch::new(("feature", ix), title, blurb, on)
                .first(ix == 0)
                .glyph(glyph)
                .truncate()
                .badge(badge),
            cx,
            move |this, cx| {
                this.workspace
                    .update(cx, |workspace, cx| workspace.set_feature(feature, !on, cx));
            },
        )
    }
}
