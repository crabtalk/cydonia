//! The agents section without the `desktop` feature: no registry to list and
//! nothing to install into, so the section says where that happens.

use crate::view::settings::SettingsWindow;
use bezel::{
    gpui::{AnyElement, Context, div, prelude::*},
    theme::{TextStyle, Theme, Typeset},
    ui::widgets::Scaffolding as _,
};

impl SettingsWindow {
    pub(super) fn load(&mut self, cx: &mut Context<Self>) {
        let _ = cx;
    }

    pub(super) fn trust_dialog(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let _ = cx;
        None
    }

    pub(super) fn agents_body(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = Theme::of(cx).clone();
        theme
            .group_box()
            .child(
                theme.card_row(true).child(
                    div()
                        .text_style(TextStyle::Callout)
                        .text_color(theme.text_muted)
                        .child(
                            "Agents are installed and run by the desktop app. This demo \
                             has one stand-in, Demo agent, which answers every session.",
                        ),
                ),
            )
            .into_any_element()
    }
}
