//! The settings page over the window, where there is no second window to open
//! it in. Built only without the `desktop` feature.

use crate::view::{settings::SettingsWindow, window::root::Cydonia, window::section::Section};
use bezel::{
    gpui::{
        App, Context, DismissEvent, Entity, EventEmitter, FocusHandle, Focusable, Window, div,
        prelude::*, px,
    },
    theme::Theme,
    ui::{icons, widgets::Buttons as _},
};

/// The settings page and the frame it is shown in.
pub(crate) struct Sheet {
    page: Entity<SettingsWindow>,
    focus: FocusHandle,
}

impl Cydonia {
    /// Put the settings up over the window, open at `section`.
    pub(crate) fn settings_sheet(
        &mut self,
        section: Section,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.menu = None;
        let workspace = self.workspace.clone();
        self.toggle_modal(window, cx, |_, cx| Sheet {
            page: crate::view::settings::embed(workspace, section, cx),
            focus: cx.focus_handle(),
        });
    }
}

impl EventEmitter<DismissEvent> for Sheet {}

impl Focusable for Sheet {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Render for Sheet {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = Theme::of(cx).clone();
        div()
            .id("settings-scrim")
            .track_focus(&self.focus)
            .absolute()
            .inset_0()
            .flex()
            .items_center()
            .justify_center()
            .p(px(24.))
            .bg(theme.scrim())
            .on_click(cx.listener(|_, _, _, cx| cx.emit(DismissEvent)))
            .child(
                div()
                    .id("settings-sheet")
                    .w_full()
                    .h_full()
                    .max_w(px(900.))
                    .max_h(px(620.))
                    .overflow_hidden()
                    .rounded(px(Theme::panel_radius()))
                    .border_1()
                    .border_color(theme.border)
                    .relative()
                    .on_click(|_, _, cx| cx.stop_propagation())
                    .child(self.page.clone())
                    .child(
                        theme
                            .ghost("settings-close")
                            .absolute()
                            .top(px(10.))
                            .right(px(10.))
                            .p(px(6.))
                            .tooltip(|window, cx| {
                                bezel::ui::tooltip::Tooltip::text("Close", window, cx)
                            })
                            .child(
                                icons::icon(icons::notifications::X)
                                    .size(px(14.))
                                    .text_color(theme.text_muted),
                            )
                            .on_click(cx.listener(|_, _, _, cx| cx.emit(DismissEvent))),
                    ),
            )
    }
}
