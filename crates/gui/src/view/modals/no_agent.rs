//! What a session asked for with no agent installed raises: the notice, and
//! the way to Settings › Agents.

use crate::view::{
    detail::{adrift, agent_missing},
    kit::dialog::{answers, dialog},
    window::{root::Cydonia, section::Section},
};
use bezel::{
    gpui::{
        App, Context, DismissEvent, EventEmitter, FocusHandle, Focusable, WeakEntity, Window, div,
        prelude::*,
    },
    theme::Theme,
    ui::popover,
};

/// The notice for a session asked for with no agent to open it on.
pub(crate) struct NoAgent {
    root: WeakEntity<Cydonia>,
    focus: FocusHandle,
}

impl Cydonia {
    /// Say there is no agent to open a session on, and offer the install.
    pub(crate) fn no_agent(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.close_menu(cx);
        let root = cx.entity().downgrade();
        self.toggle_modal(window, cx, |_, cx| NoAgent {
            root,
            focus: cx.focus_handle(),
        });
    }
}

impl EventEmitter<DismissEvent> for NoAgent {}

impl Focusable for NoAgent {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Render for NoAgent {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = Theme::of(cx).clone();
        let dialog = dialog(
            "no-agent-dialog",
            &theme,
            window,
            [
                popover::dialog_title(&theme, agent_missing(None)).into_any_element(),
                popover::dialog_body(&theme, adrift(None, false)).into_any_element(),
                answers(
                    &theme,
                    "no-agent",
                    Some(Box::new(cx.listener(|_, _, _, cx| cx.emit(DismissEvent)))),
                    "Install an agent…",
                    Box::new(cx.listener(|this, _, window, cx| {
                        cx.emit(DismissEvent);
                        let root = this.root.clone();
                        window.defer(cx, move |window, cx| {
                            root.update(cx, |root, cx| {
                                root.open_settings(Section::Agents, window, cx)
                            })
                            .ok();
                        });
                    })),
                )
                .into_any_element(),
            ],
            cx.listener(|_, _, _, cx| cx.emit(DismissEvent)),
        );
        div().track_focus(&self.focus).child(dialog)
    }
}
