//! What's New: the latest release's notes over the window, raised on the
//! first launch after they change, and again from the app menu or Settings ›
//! General. The shell is here; what a release shows is `release`.

pub(crate) mod release;

use crate::{
    model::{version::Version, workspace::Workspace},
    view::{
        kit::dialog::{answers, dialog},
        window::root::{Cydonia, WhatsNew},
    },
};
use bezel::{
    gpui::{
        App, Context, DismissEvent, Entity, EventEmitter, FocusHandle, Focusable, Window, div,
        prelude::*,
    },
    theme::Theme,
    ui::popover,
};

/// The notes, over the window.
pub(crate) struct Notes {
    body: Entity<release::Body>,
    focus: FocusHandle,
}

impl Cydonia {
    /// Raise What's New, and record its notes as shown.
    pub(crate) fn whats_new(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.workspace.update(cx, |workspace, _| {
            workspace.saw_release(Version::new(release::VERSION))
        });
        self.close_menu(cx);
        let workspace = self.workspace.clone();
        self.toggle_modal(window, cx, |_, cx| Notes::new(workspace, cx));
    }

    pub(crate) fn whats_new_action(
        &mut self,
        _: &WhatsNew,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.whats_new(window, cx);
    }

    /// [`Self::whats_new`], once per version of the notes.
    pub(crate) fn whats_new_once(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let notes = Version::new(release::VERSION);
        if self.workspace.read(cx).release_unseen(&notes) {
            self.whats_new(window, cx);
        }
    }
}

impl Notes {
    fn new(workspace: Entity<Workspace>, cx: &mut Context<Self>) -> Self {
        Self {
            body: cx.new(|cx| release::Body::new(workspace, cx)),
            focus: cx.focus_handle(),
        }
    }
}

impl EventEmitter<DismissEvent> for Notes {}

impl Focusable for Notes {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Render for Notes {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = Theme::of(cx).clone();
        let mut content = vec![
            popover::dialog_title(&theme, format!("What's new in {}", release::VERSION))
                .into_any_element(),
            popover::dialog_body(&theme, release::HEADLINE).into_any_element(),
        ];
        content.push(self.body.clone().into_any_element());
        content.push(
            answers(
                &theme,
                "whats-new",
                None,
                "Done",
                Box::new(cx.listener(|_, _, _, cx| cx.emit(DismissEvent))),
            )
            .into_any_element(),
        );
        let dialog = dialog(
            "whats-new-dialog",
            &theme,
            true,
            window,
            content,
            cx.listener(|_, _, _, cx| cx.emit(DismissEvent)),
        );
        div().track_focus(&self.focus).child(dialog)
    }
}
