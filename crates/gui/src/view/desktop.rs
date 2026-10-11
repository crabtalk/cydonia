//! The answer a control gives where the build has nothing behind it: the
//! control stays on screen, and pressing it says the desktop app has it.
//! Raised only without the `desktop` feature.

use crate::view::{
    kit::dialog::{answers, dialog},
    root::Cydonia,
};
use bezel::{
    gpui::{AnyElement, Context, Window, prelude::*},
    theme::Theme,
    ui::popover,
};

/// Something asked for that only the desktop app can do, by what it is
/// called — raised by a view that cannot reach the window.
pub struct DesktopOnly(pub &'static str);

impl Cydonia {
    /// Say that `what` needs the desktop app. `what` opens the sentence.
    pub(crate) fn desktop_only(&mut self, what: &'static str, cx: &mut Context<Self>) {
        self.menu = None;
        self.desktop_only = Some(what);
        cx.notify();
    }

    fn dismiss_desktop_only(&mut self, cx: &mut Context<Self>) {
        self.desktop_only = None;
        cx.notify();
    }

    /// The notice, over a scrim that takes the press dismissing it.
    pub(crate) fn desktop_only_notice(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let what = self.desktop_only?;
        let theme = Theme::of(cx).clone();
        Some(dialog(
            "desktop-only-dialog",
            &theme,
            window,
            [
                popover::dialog_title(&theme, format!("{what} works in the desktop app"))
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
                    Box::new(cx.listener(|this, _, _, cx| this.dismiss_desktop_only(cx))),
                )
                .into_any_element(),
            ],
            cx.listener(|this, _, _, cx| this.dismiss_desktop_only(cx)),
        ))
    }
}
