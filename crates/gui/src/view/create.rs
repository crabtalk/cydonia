//! The question asked before a board is made: what to call it, and the key its
//! cards will carry.
//!
//! Asked rather than assumed. A board is keyed for as long as it exists — the
//! handles already written down go on meaning the cards they meant — so the one
//! moment the key is free is the moment before there are any, and that is this
//! one.

use crate::view::{
    kit::{
        dialog::{answers, dialog},
        form,
    },
    root::Cydonia,
};
use artifact::board::{self, key};
use artifact::space::Member;
use bezel::{
    gpui::{
        self, AnyElement, App, Context, Entity, Focusable as _, KeyBinding, SharedString,
        Subscription, Window, actions, prelude::*,
    },
    theme::Theme,
    ui::{
        input::{FieldEvent, TextField},
        popover,
    },
};
use std::collections::HashSet;

actions!(cydonia_create, [CommitBoard, DismissBoard]);

/// Claimed on the dialog's fields, so `enter` makes the board.
const CREATE_CONTEXT: &str = "CydoniaNewBoard";

pub fn bindings() -> Vec<KeyBinding> {
    vec![
        KeyBinding::new("enter", CommitBoard, Some(CREATE_CONTEXT)),
        KeyBinding::new("escape", DismissBoard, Some(CREATE_CONTEXT)),
    ]
}

/// A board that has been asked for and not yet made. Nothing is on disk until
/// Create: dismissing leaves the project as it was.
pub(crate) struct Making {
    /// Which project it lands in. Carried rather than taken from whichever is
    /// active, so the dialog makes the board under the heading its `+` was
    /// pressed on.
    pub project: usize,
    /// The pane whose `+` asked for it: the board opens there as a tab.
    pub into: Option<Member>,
    pub name: Entity<TextField>,
    pub key: Entity<TextField>,
    /// The key as last derived from the name. While the field still holds it
    /// the key follows what is being typed; the moment it is typed over, it is
    /// the person's and stays put.
    pub derived: SharedString,
    /// What was wrong last time. Keeps the dialog open: a taken key is a thing
    /// to fix, not to be told about afterwards.
    pub error: Option<SharedString>,
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
        self.ask_new_board_into(project, None, window, cx);
    }

    /// [`Self::ask_new_board`], opening the board as a tab of `into`.
    pub(crate) fn ask_new_board_into(
        &mut self,
        project: usize,
        into: Option<Member>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.commit(cx);
        // One editor at a time, as [`Cydonia::open_info`] keeps it.
        self.menu = None;
        self.renaming = None;
        self.info = None;
        let derived = self.derived_key(project, "", cx);
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
        window.focus(&name.read(cx).focus_handle(cx), cx);
        self.making = Some(Making {
            project,
            into,
            name,
            key,
            derived,
            error: None,
            _watch: watch,
        });
        cx.notify();
    }

    /// Keep the key following the name, for as long as the key is still the
    /// one this put there.
    fn follow_key(&mut self, name: &str, cx: &mut Context<Self>) {
        let Some((project, field)) = self.making.as_ref().and_then(|making| {
            (making.key.read(cx).content() == &making.derived)
                .then(|| (making.project, making.key.clone()))
        }) else {
            return;
        };
        let derived = self.derived_key(project, name, cx);
        field.update(cx, |field, cx| field.set_content(derived.clone(), cx));
        if let Some(making) = self.making.as_mut() {
            making.derived = derived;
        }
    }

    /// The key a board of this name would take in this project, clear of the
    /// keys its neighbours hold. A board still unnamed is keyed off the name it
    /// would be filed under, so the field never stands empty or shows the
    /// fallback for a name nobody has typed yet.
    fn derived_key(&self, project: usize, name: &str, cx: &App) -> SharedString {
        let taken: HashSet<String> = self
            .workspace
            .read(cx)
            .projects
            .get(project)
            .map(|open| open.boards.iter().map(|board| board.key.clone()).collect())
            .unwrap_or_default();
        let name = match name.trim().is_empty() {
            true => board::NAMED,
            false => name.trim(),
        };
        key::derive(name, &taken).into()
    }

    pub(crate) fn make_board(
        &mut self,
        _: &CommitBoard,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(making) = self.making.as_ref() else {
            return;
        };
        let project = making.project;
        let into = making.into.clone();
        let key = making.key.read(cx).content().trim().to_owned();
        // A board left unnamed is the one the old `+` made outright — see
        // [`board::NAMED`]. Nothing else in the sidebar would tell them apart.
        let name = match making.name.read(cx).content().trim() {
            "" => board::NAMED.to_owned(),
            typed => typed.to_owned(),
        };
        let made = self.workspace.update(cx, |workspace, cx| {
            workspace.new_board(project, name, &key, cx)
        });
        match made {
            Ok(ix) => {
                self.making = None;
                let member = self.workspace.read(cx).board_member(project, ix);
                match into.zip(member) {
                    Some((pane, member)) => self.add_tab(&pane, member, window, cx),
                    None => self.open_board(project, ix, window, cx),
                }
            }
            // Left open, holding what was typed.
            Err(why) => {
                if let Some(making) = self.making.as_mut() {
                    making.error = Some(why.into());
                }
            }
        }
        cx.notify();
    }

    pub(crate) fn dismiss_new_board(
        &mut self,
        _: &DismissBoard,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.making = None;
        cx.notify();
    }

    /// The dialog, over a scrim that takes the press dismissing it.
    pub(crate) fn new_board_dialog(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let making = self.making.as_ref()?;
        let theme = Theme::of(cx).clone();
        Some(dialog(
            "new-board-dialog",
            &theme,
            window,
            [
                popover::dialog_title(&theme, "New board").into_any_element(),
                form::row(&theme, "Name", making.name.clone()),
                form::row(&theme, "Key", making.key.clone()),
            ]
            .into_iter()
            // Only on an error: a standing hint would caption two labelled
            // fields, and any example would name another board.
            .chain(
                making
                    .error
                    .clone()
                    .map(|why| form::error(&theme, why).into_any_element()),
            )
            .chain([answers(
                &theme,
                "new-board",
                Some(Box::new(cx.listener(|this, _, window, cx| {
                    this.dismiss_new_board(&DismissBoard, window, cx)
                }))),
                "Create",
                Box::new(
                    cx.listener(|this, _, window, cx| this.make_board(&CommitBoard, window, cx)),
                ),
            )
            .into_any_element()]),
            cx.listener(|this, _, window, cx| this.dismiss_new_board(&DismissBoard, window, cx)),
        ))
    }
}
