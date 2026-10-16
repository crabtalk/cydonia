//! The latest release's notes. Rewritten, [`VERSION`] included, for a release
//! that has something to show; a release that leaves this file alone raises
//! nothing on launch.

use crate::{model::workspace::Workspace, view::kit};
use bezel::{
    gpui::{AnyElement, App, Entity, div, prelude::*},
    theme::{TextStyle, Theme, Typeset},
};

/// The release these notes are for.
pub(crate) const VERSION: &str = "0.2.2";

/// The line under the title.
pub(crate) const HEADLINE: &str = "What is open now floats on a card over the window. Floating is \
     the new default layout; pick Flat to go back to the window as it was.";

/// What follows [`HEADLINE`].
pub(super) fn body(theme: &Theme, workspace: &Entity<Workspace>, cx: &App) -> Vec<AnyElement> {
    vec![
        div()
            .py_2()
            .flex()
            .justify_center()
            .child(kit::layout::picker(
                "whats-new-layout",
                theme,
                workspace,
                cx,
            ))
            .into_any_element(),
        div()
            .text_style(TextStyle::Callout)
            .text_color(theme.text_muted)
            .child("Change it any time in Settings › Appearance › Window.")
            .into_any_element(),
    ]
}
