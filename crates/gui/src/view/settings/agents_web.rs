//! The agents section without the `desktop` feature: no registry to list and
//! nothing to install into, so the section says where that happens.

use crate::view::settings::{SettingsWindow, note};
use bezel::{
    gpui::{AnyElement, Context, Window, prelude::*},
    theme::Theme,
    ui::widgets::Scaffolding as _,
};

impl SettingsWindow {
    pub(super) fn load(&mut self, cx: &mut Context<Self>) {
        let _ = cx;
    }

    pub(super) fn trust_dialog(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let _ = (window, cx);
        None
    }

    pub(super) fn agents_body(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = Theme::of(cx).clone();
        theme
            .group_box()
            .child(note(
                &theme,
                "Agents are installed and run by the desktop app. This demo has one \
                     stand-in, Demo agent, which answers every session.",
                true,
            ))
            .into_any_element()
    }
}
