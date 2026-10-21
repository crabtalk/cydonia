//! The answer a control gives where the build has nothing behind it: the
//! control stays on screen, and pressing it says the desktop app has it.
//! Raised only without the `desktop` feature.

use crate::view::{
    kit::dialog::{answers, dialog},
    window::root::Cydonia,
};
use bezel::{
    gpui::{
        App, Context, DismissEvent, EventEmitter, FocusHandle, Focusable, Window, div, prelude::*,
    },
    theme::Theme,
    ui::popover,
};

/// Something asked for that only the desktop app can do, by what it is
/// called — raised by a view that cannot reach the window.
pub struct DesktopOnly(pub &'static str);

/// The notice for a [`DesktopOnly`].
pub(crate) struct Notice {
    /// What was asked for. Opens the sentence.
    what: &'static str,
    focus: FocusHandle,
}

impl Cydonia {
    /// Say that `what` needs the desktop app. `what` opens the sentence.
    pub(crate) fn desktop_only(
        &mut self,
        what: &'static str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.close_menu(cx);
        self.toggle_modal(window, cx, |_, cx| Notice {
            what,
            focus: cx.focus_handle(),
        });
    }
}

impl EventEmitter<DismissEvent> for Notice {}

impl Focusable for Notice {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Render for Notice {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = Theme::of(cx).clone();
        let dialog = dialog(
            "desktop-only-dialog",
            &theme,
            false,
            window,
            [
                popover::dialog_title(&theme, format!("{} works in the desktop app", self.what))
                    .into_any_element(),
                popover::dialog_body(
                    &theme,
                    "This is a demo in the browser, with no disk, shell or agents behind it. \
                     Download Cydonia to use it.",
                )
                .into_any_element(),
                answers(
                    &theme,
                    "desktop-only",
                    None,
                    "OK",
                    Box::new(cx.listener(|_, _, _, cx| cx.emit(DismissEvent))),
                )
                .into_any_element(),
            ],
            cx.listener(|_, _, _, cx| cx.emit(DismissEvent)),
        );
        div().track_focus(&self.focus).child(dialog)
    }
}
