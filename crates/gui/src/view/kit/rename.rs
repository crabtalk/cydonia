//! A name typed over what it names, in its place: `enter` keeps it, `escape`
//! drops it, and a press anywhere else keeps it too — the name typed is the
//! name meant.

use bezel::{
    gpui::{
        self, Context, Entity, EventEmitter, FocusHandle, Focusable, KeyBinding, MouseButton,
        Window, actions, div, prelude::*,
    },
    ui::input::{Case, TextField},
};

/// Claimed on the field so `enter` keeps the name and `escape` drops it.
const RENAME_CONTEXT: &str = "CydoniaSessionName";

actions!(cydonia_rename, [CommitName, DismissName]);

pub fn bindings() -> Vec<KeyBinding> {
    vec![
        KeyBinding::new("enter", CommitName, Some(RENAME_CONTEXT)),
        KeyBinding::new("escape", DismissName, Some(RENAME_CONTEXT)),
    ]
}

/// What became of a name being typed.
pub(crate) enum NameEvent {
    /// Kept, as typed.
    Named(String),
    /// Dropped.
    Dismissed,
}

/// The field a name is typed in.
pub(crate) struct NameField {
    field: Entity<TextField>,
}

impl EventEmitter<NameEvent> for NameField {}

impl Focusable for NameField {
    fn focus_handle(&self, cx: &gpui::App) -> FocusHandle {
        self.field.focus_handle(cx)
    }
}

impl NameField {
    pub(crate) fn new(cx: &mut Context<Self>) -> Self {
        Self {
            field: cx.new(|cx| {
                TextField::new(cx)
                    .with_frame(false)
                    .with_key_context(RENAME_CONTEXT)
                    .with_placeholder("name this session…")
            }),
        }
    }

    /// Put `label` in the field, in `case`, and take the caret to it.
    pub(crate) fn start(
        &mut self,
        label: String,
        case: Case,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.field.update(cx, |field, cx| {
            field.set_case(case);
            field.set_content(label, cx);
        });
        window.focus(&self.field.focus_handle(cx), cx);
    }

    fn named(&mut self, cx: &mut Context<Self>) {
        let name = self.field.read(cx).content().to_string();
        cx.emit(NameEvent::Named(name));
    }
}

impl Render for NameField {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // It carries its own press: `TextField` does not focus itself, and a
        // press that reached the row would open what is being named out from
        // under the name.
        div()
            .flex_1()
            .min_w_0()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, window, cx| {
                    cx.stop_propagation();
                    window.focus(&this.field.focus_handle(cx), cx);
                }),
            )
            .on_mouse_down_out(cx.listener(|this, _, _, cx| this.named(cx)))
            .on_action(cx.listener(|this, _: &CommitName, _, cx| this.named(cx)))
            .on_action(cx.listener(|_, _: &DismissName, _, cx| cx.emit(NameEvent::Dismissed)))
            .child(self.field.clone())
    }
}
