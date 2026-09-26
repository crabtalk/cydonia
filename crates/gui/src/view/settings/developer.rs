//! The developer section: switches for looking at things that have not
//! happened.
//!
//! Nothing here changes what the app does with a project, and nothing here is
//! written to `settings.toml` — a switch for looking at something is not a
//! preference, and a relaunch is the right way to put every one of them down.
//! The section is absent from a release build altogether; see
//! [`super::Section::listed`].

#[cfg(feature = "desktop")]
use crate::model::update;
use crate::view::settings::SettingsWindow;
#[cfg(feature = "desktop")]
use crate::view::settings::Switch;
use bezel::gpui::{AnyElement, Context, div, prelude::*, px};
#[cfg(feature = "desktop")]
use bezel::{
    theme::Theme,
    ui::{icons, widgets::Scaffolding},
};

impl SettingsWindow {
    pub(super) fn developer_body(&self, cx: &Context<Self>) -> AnyElement {
        div()
            .flex()
            .flex_col()
            .gap(px(super::GROUP_GAP))
            .children(self.notifier_row(cx))
            .into_any_element()
    }

    /// The notifier, on demand. A release has to be published, found, fetched
    /// and verified before that line appears on its own, which makes it the one
    /// piece of chrome in the app that cannot be looked at by using the app —
    /// hence a switch for it.
    ///
    /// What it puts up is the real notifier in its real place, not a picture of
    /// one: the sidebar reads the same updater. It is inert, though — see
    /// [`crate::model::update::Updater::ready`] — so the app does not restart
    /// out from under whoever is looking at it.
    #[cfg(not(feature = "desktop"))]
    fn notifier_row(&self, cx: &Context<Self>) -> Option<AnyElement> {
        let _ = cx;
        None
    }

    #[cfg(feature = "desktop")]
    fn notifier_row(&self, cx: &Context<Self>) -> Option<AnyElement> {
        let updater = update::of(cx)?;
        let theme = Theme::of(cx).clone();
        let on = updater.read(cx).previewing();
        Some(
            theme
                .group_box()
                .child(
                    self.switch_row(
                        Switch::new(
                            "preview-notifier",
                            "Update notifier",
                            "Put the restart notice at the foot of the sidebar, where a release \
                         would.",
                            on,
                        )
                        .first(true)
                        .glyph(icons::development::CircleFadingArrowUp)
                        .truncate(),
                        cx,
                        move |_, cx| {
                            updater.update(cx, |updater, cx| updater.set_preview(!on, cx));
                        },
                    ),
                )
                .into_any_element(),
        )
    }
}
