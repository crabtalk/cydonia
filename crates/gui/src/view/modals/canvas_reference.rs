use crate::{
    model::workspace::Workspace,
    view::{
        canvas::CanvasView,
        kit::{dialog, form, link::mention},
        window::root::Cydonia,
    },
};
use bezel::{
    gpui::{
        self, App, Context, DismissEvent, Entity, EventEmitter, FocusHandle, Focusable, KeyBinding,
        SharedString, Subscription, WeakEntity, Window, div, prelude::*, px,
    },
    theme::Theme,
    ui::{
        input::{FieldEvent, TextField},
        popover, scroll,
        widgets::Button,
    },
};

const CONTEXT: &str = "CydoniaCanvasReference";
gpui::actions!(cydonia_canvas_reference, [SubmitCanvasReference]);

pub(crate) fn bindings() -> Vec<KeyBinding> {
    vec![KeyBinding::new(
        "enter",
        SubmitCanvasReference,
        Some(CONTEXT),
    )]
}

pub(crate) struct CanvasReference {
    target: WeakEntity<CanvasView>,
    replace: Option<String>,
    field: Entity<TextField>,
    workspace: Entity<Workspace>,
    error: Option<String>,
    _field: Subscription,
}
impl EventEmitter<DismissEvent> for CanvasReference {}
impl Focusable for CanvasReference {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.field.read(cx).focus_handle(cx)
    }
}
impl Cydonia {
    pub(crate) fn pick_canvas_reference(
        &mut self,
        target: WeakEntity<CanvasView>,
        initial: String,
        replace: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let workspace = self.workspace.clone();
        self.toggle_modal(window, cx, |_, cx| {
            let field = form::field(CONTEXT, initial, "Search entries or enter #12#heading…", cx);
            let changed = cx.subscribe(
                &field,
                |this: &mut CanvasReference, _, event: &FieldEvent, cx| {
                    if matches!(event, FieldEvent::Changed(_)) {
                        this.error = None;
                        cx.notify();
                    }
                },
            );
            CanvasReference {
                target,
                replace,
                field,
                workspace,
                error: None,
                _field: changed,
            }
        });
    }
}
impl CanvasReference {
    fn submit(&mut self, cx: &mut Context<Self>) {
        let text = self.field.read(cx).content().trim().to_owned();
        let replace = self.replace.clone();
        let result = self
            .target
            .update(cx, |target, cx| target.reference(&text, replace, cx));
        match result {
            Ok(Ok(())) => cx.emit(DismissEvent),
            Ok(Err(error)) => {
                self.error = Some(error);
                cx.notify();
            }
            Err(_) => cx.emit(DismissEvent),
        }
    }
}
impl Render for CanvasReference {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = Theme::of(cx).clone();
        let held = mention::read(self.workspace.read(cx), cx);
        let query = mention::Query::parse(self.field.read(cx).content());
        let rows = query
            .rank(&held.0)
            .into_iter()
            .map(|at| {
                let item = &held.0[at];
                let url = item.url.clone();
                Button::new(
                    SharedString::from(format!("canvas-reference-{}", item.url)),
                    item.title.clone(),
                )
                .on_press(cx.listener(move |this, _, _, cx| {
                    this.field
                        .update(cx, |field, cx| field.set_content(url.clone(), cx));
                }))
                .into_any_element()
            })
            .collect::<Vec<_>>();
        let list = div().flex().flex_col().gap(px(4.)).children(rows);
        let content = vec![
            popover::dialog_title(
                &theme,
                if self.replace.is_some() {
                    "Replace entry card"
                } else {
                    "Add entry card"
                },
            )
            .into_any_element(),
            self.field.clone().into_any_element(),
            scroll::Viewport::new(
                "canvas-entry-options",
                list.id("canvas-reference-rows").h(px(280.)),
                bezel::gpui::Axis::Vertical,
            )
            .into_any_element(),
            form::error(&theme, self.error.clone().unwrap_or_default().into()).into_any_element(),
            dialog::answers(
                &theme,
                "canvas-reference",
                Some(Box::new(cx.listener(|_, _, _, cx| cx.emit(DismissEvent)))),
                if self.replace.is_some() {
                    "Replace"
                } else {
                    "Add"
                },
                Box::new(cx.listener(|this, _, _, cx| this.submit(cx))),
            )
            .into_any_element(),
        ];
        div()
            .on_action(cx.listener(|this, _: &SubmitCanvasReference, _, cx| this.submit(cx)))
            .child(dialog::dialog(
                "canvas-reference-dialog",
                &theme,
                false,
                window,
                content,
                cx.listener(|_, _, _, cx| cx.emit(DismissEvent)),
            ))
    }
}
