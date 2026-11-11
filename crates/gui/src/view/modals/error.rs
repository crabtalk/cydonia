use crate::view::{kit::dialog, window::root::Cydonia};
use bezel::{
    gpui::{
        App, Context, DismissEvent, EventEmitter, FocusHandle, Focusable, Window, div, prelude::*,
    },
    theme::Theme,
    ui::popover,
};

pub(crate) struct ErrorModal {
    title: String,
    message: String,
    focus: FocusHandle,
}
impl EventEmitter<DismissEvent> for ErrorModal {}
impl Focusable for ErrorModal {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Cydonia {
    pub(crate) fn show_error(
        &mut self,
        title: String,
        message: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.toggle_modal(window, cx, |_, cx| ErrorModal {
            title,
            message,
            focus: cx.focus_handle(),
        });
    }
}

impl Render for ErrorModal {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = Theme::of(cx).clone();
        div().track_focus(&self.focus).child(dialog::dialog(
            "error-dialog",
            &theme,
            false,
            window,
            [
                popover::dialog_title(&theme, self.title.clone()).into_any_element(),
                popover::dialog_body(&theme, self.message.clone()).into_any_element(),
                dialog::answers(
                    &theme,
                    "error",
                    None,
                    "OK",
                    Box::new(cx.listener(|_, _, _, cx| cx.emit(DismissEvent))),
                )
                .into_any_element(),
            ],
            cx.listener(|_, _, _, cx| cx.emit(DismissEvent)),
        ))
    }
}
