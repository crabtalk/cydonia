//! The question asked before a board is made: what to call it, and the key its
//! cards will carry.
//!
//! Asked rather than assumed. A board is keyed for as long as it exists — the
//! handles already written down go on meaning the cards they meant — so the one
//! moment the key is free is the moment before there are any, and that is this
//! one.

use crate::model::workspace::Opening;
use crate::{
    model::workspace::Workspace,
    view::{
        kit::{
            dialog::{answers, dialog},
            form,
        },
        window::root::Cydonia,
    },
};
use artifact::board;
use bezel::{
    gpui::{
        self, App, Context, DismissEvent, Entity, EventEmitter, FocusHandle, Focusable, KeyBinding,
        SharedString, Subscription, WeakEntity, Window, actions, div, prelude::*,
    },
    theme::Theme,
    ui::{
        input::{FieldEvent, TextField},
        popover,
    },
};

/// Claimed on the dialog's fields, so `enter` makes the board.
const CREATE_CONTEXT: &str = "CydoniaNewBoard";

actions!(cydonia_create, [CommitBoard]);

pub fn bindings() -> Vec<KeyBinding> {
    vec![KeyBinding::new("enter", CommitBoard, Some(CREATE_CONTEXT))]
}

/// A board that has been asked for and not yet made, as a modal. Nothing is on
/// disk until Create: dismissing leaves the project as it was.
pub(crate) struct NewBoard {
    /// Which project it lands in: the one under whose heading it was asked
    /// for, not whichever is active.
    project: usize,
    name: Entity<TextField>,
    key: Entity<TextField>,
    /// The key as last derived from the name. While the field still holds it
    /// the key follows what is being typed; once it is typed over, it stays.
    derived: SharedString,
    /// What was wrong last time. The dialog stays open while there is one.
    error: Option<SharedString>,
    workspace: Entity<Workspace>,
    root: WeakEntity<Cydonia>,
    /// Holds the name field's subscription for as long as the dialog is up.
    _watch: Subscription,
}

impl Cydonia {
    /// Raise the question. Every New Board comes through here — the sidebar's
    /// `+` and the menu bar both — so a board is never made unnamed.
    pub(crate) fn ask_new_board(
        &mut self,
        project: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.commit(cx);
        self.close_menu(cx);
        let workspace = self.workspace.clone();
        let root = cx.entity().downgrade();
        self.toggle_modal(window, cx, |_, cx| {
            NewBoard::new(project, workspace, root, cx)
        });
    }
}

impl NewBoard {
    fn new(
        project: usize,
        workspace: Entity<Workspace>,
        root: WeakEntity<Cydonia>,
        cx: &mut Context<Self>,
    ) -> Self {
        let derived: SharedString = workspace.read(cx).free_key(project, "", cx).into();
        let name = form::field(
            CREATE_CONTEXT,
            SharedString::default(),
            "name this board…",
            cx,
        );
        let key = form::field(CREATE_CONTEXT, derived.clone(), "KEY", cx);
        // Subscribed rather than observed: a field notifies on its own caret
        // blink, and the key would be rederived twice a second.
        let watch = cx.subscribe(&name, |this, field, event: &FieldEvent, cx| {
            if matches!(event, FieldEvent::Changed(_)) {
                let typed = field.read(cx).content().clone();
                this.follow_key(&typed, cx);
            }
        });
        Self {
            project,
            name,
            key,
            derived,
            error: None,
            workspace,
            root,
            _watch: watch,
        }
    }

    /// Keep the key following the name, for as long as the key is still the
    /// one this put there.
    fn follow_key(&mut self, name: &str, cx: &mut Context<Self>) {
        if self.key.read(cx).content() != &self.derived {
            return;
        }
        let derived: SharedString = self
            .workspace
            .read(cx)
            .free_key(self.project, name, cx)
            .into();
        self.key
            .update(cx, |field, cx| field.set_content(derived.clone(), cx));
        self.derived = derived;
    }

    fn make(&mut self, _: &CommitBoard, window: &mut Window, cx: &mut Context<Self>) {
        let project = self.project;
        let key = self.key.read(cx).content().trim().to_owned();
        // A board left unnamed is the one the old `+` made outright — see
        // [`board::NAMED`]. Nothing else in the sidebar would tell them apart.
        let name = match self.name.read(cx).content().trim() {
            "" => board::NAMED.to_owned(),
            typed => typed.to_owned(),
        };
        let made = self.workspace.update(cx, |workspace, cx| {
            workspace.new_board(project, name, &key, cx)
        });
        match made {
            Ok(ix) => {
                let _ = self.root.update(cx, |root, cx| {
                    root.open_board(project, ix, Opening::Kept, window, cx)
                });
                cx.emit(DismissEvent);
            }
            // Left open, holding what was typed.
            Err(why) => {
                self.error = Some(why.into());
                cx.notify();
            }
        }
    }
}

impl EventEmitter<DismissEvent> for NewBoard {}

impl Focusable for NewBoard {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.name.focus_handle(cx)
    }
}

impl Render for NewBoard {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = Theme::of(cx).clone();
        let dialog = dialog(
            "new-board-dialog",
            &theme,
            false,
            window,
            [
                popover::dialog_title(&theme, "New board").into_any_element(),
                form::row(&theme, "Name", self.name.clone()),
                form::row(&theme, "Key", self.key.clone()),
            ]
            .into_iter()
            // Only on an error: a standing hint would caption two labelled
            // fields, and any example would name another board.
            .chain(
                self.error
                    .clone()
                    .map(|why| form::error(&theme, why).into_any_element()),
            )
            .chain([answers(
                &theme,
                "new-board",
                Some(Box::new(cx.listener(|_, _, _, cx| cx.emit(DismissEvent)))),
                "Create",
                Box::new(cx.listener(|this, _, window, cx| this.make(&CommitBoard, window, cx))),
            )
            .into_any_element()]),
            cx.listener(|_, _, _, cx| cx.emit(DismissEvent)),
        );
        div().on_action(cx.listener(Self::make)).child(dialog)
    }
}
