//! The answer a control gives where the build has nothing behind it: the
//! control stays on screen, and pressing it says the desktop app has it.
//! Raised only without the `desktop` feature.

use crate::view::root::Cydonia;
use bezel::{
    gpui::{AnyElement, Context, div, prelude::*, px},
    theme::{TextStyle, Theme, Typeset},
    ui::widgets::{ButtonStyle, Buttons, Scaffolding as _},
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

    /// The notice, over a scrim that takes the press dismissing it — the same
    /// shape as [`Cydonia::confirm_delete`].
    pub(crate) fn desktop_only_notice(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let what = self.desktop_only?;
        let theme = Theme::of(cx).clone();
        Some(
            div()
                .id("desktop-only-scrim")
                .absolute()
                .inset_0()
                .flex()
                .items_center()
                .justify_center()
                .bg(theme.scrim())
                .on_click(cx.listener(|this, _, _, cx| this.dismiss_desktop_only(cx)))
                .child(
                    div()
                        .id("desktop-only-dialog")
                        .w(px(360.))
                        .flex()
                        .flex_col()
                        .gap(px(8.))
                        .p(px(20.))
                        .rounded(px(Theme::panel_radius()))
                        .border_1()
                        .border_color(theme.border)
                        .bg(theme.surface)
                        .on_click(|_, _, cx| cx.stop_propagation())
                        .child(theme.row_title(format!("{what} works in the desktop app")))
                        .child(
                            div()
                                .text_style(TextStyle::Subheadline)
                                .text_color(theme.text_muted)
                                .child(
                                    "This is a demo in the browser, with no disk, shell or \
                                     agents behind it. Download Cydonia to use it.",
                                ),
                        )
                        .child(
                            div().mt(px(4.)).flex().flex_row().justify_end().child(
                                theme
                                    .button("OK", ButtonStyle::Prominent, None)
                                    .id("desktop-only-ok")
                                    .on_click(
                                        cx.listener(|this, _, _, cx| this.dismiss_desktop_only(cx)),
                                    ),
                            ),
                        ),
                )
                .into_any_element(),
        )
    }
}
