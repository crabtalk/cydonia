//! A board's identity panel: its name and the key its handles carry, edited in
//! one place because one is derived from the other.

use crate::view::{
    kit::{dialog::answers, form},
    window::root::Cydonia,
};
use bezel::{
    gpui::{
        self, AnyElement, Context, Entity, Focusable as _, KeyBinding, SharedString, Window,
        actions, prelude::*, px,
    },
    theme::Theme,
    ui::{input::TextField, popover},
};

/// Claimed on the panel's fields, so `enter` files it.
const INFO_CONTEXT: &str = "CydoniaBoardInfo";

actions!(cydonia_header, [CommitInfo, DismissInfo]);

pub fn bindings() -> Vec<KeyBinding> {
    vec![
        KeyBinding::new("enter", CommitInfo, Some(INFO_CONTEXT)),
        KeyBinding::new("escape", DismissInfo, Some(INFO_CONTEXT)),
    ]
}

/// A board's identity, open for editing: its name and the key its handles
/// carry. A panel rather than an inline field because the key is derived from
/// the name, so changing one is usually changing both.
///
/// Buffered — nothing is written until Save, and dismissing discards.
pub(crate) struct BoardInfo {
    /// Which board, by id — the panel outlives a re-read of the project.
    pub board: String,
    pub name: Entity<TextField>,
    pub key: Entity<TextField>,
    /// What was wrong last time. Keeps the panel open: a taken key is a thing
    /// to fix, not to be told about afterwards.
    pub error: Option<SharedString>,
}

impl Cydonia {
    // ── the board's identity panel ───────────────────────────────

    /// Show the panel, or put it away. The dismiss-on-press-outside lands
    /// before the click, so by then the panel already reads as shut and a plain
    /// toggle would only reopen it — what the press found is noted on the way
    /// down instead, as [`Cydonia::toggle_menu`] does.
    pub(crate) fn toggle_info(&mut self, board: &str, window: &mut Window, cx: &mut Context<Self>) {
        let shut_by_this_press = std::mem::take(&mut self.info_pressed);
        if shut_by_this_press || self.info.is_some() {
            self.info = None;
            cx.notify();
            return;
        }
        self.open_info(board, window, cx);
    }

    /// Open the panel on the board the band is showing, seeded from what it is
    /// called now.
    pub(crate) fn open_info(&mut self, board: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.commit(cx);
        // One editor at a time, and before the lookup: a rename left in flight
        // would otherwise outlive a board that has gone.
        self.menu = None;
        self.renaming = None;
        let Some((name, key)) = self
            .workspace
            .read(cx)
            .board_at(board)
            .map(|board| (board.name.clone(), board.key.clone()))
        else {
            cx.notify();
            return;
        };
        let name = form::field(INFO_CONTEXT, name, "name this board…", cx);
        let key = form::field(INFO_CONTEXT, key, "KEY", cx);
        window.focus(&name.read(cx).focus_handle(cx), cx);
        self.info = Some(BoardInfo {
            board: board.to_owned(),
            name,
            key,
            error: None,
        });
        cx.notify();
    }

    pub(crate) fn commit_info(&mut self, _: &CommitInfo, _: &mut Window, cx: &mut Context<Self>) {
        let Some(info) = self.info.as_ref() else {
            return;
        };
        let board = info.board.clone();
        let name = info.name.read(cx).content().trim().to_owned();
        let key = info.key.read(cx).content().trim().to_owned();
        let filed = self.workspace.update(cx, |workspace, cx| {
            workspace.edit_board(&board, name, &key, cx)
        });
        match filed {
            Ok(()) => self.info = None,
            // Left open, holding what was typed.
            Err(why) => {
                if let Some(info) = self.info.as_mut() {
                    info.error = Some(why.into());
                }
            }
        }
        cx.notify();
    }

    pub(crate) fn dismiss_info(&mut self, _: &DismissInfo, _: &mut Window, cx: &mut Context<Self>) {
        self.info = None;
        cx.notify();
    }

    /// The panel, hung under the name that opens it.
    pub(crate) fn info_panel(&self, board: &str, cx: &mut Context<Self>) -> Option<AnyElement> {
        let info = self.info.as_ref().filter(|info| info.board == board)?;
        let theme = Theme::of(cx).clone();
        let card = popover::popover_card(&theme)
            .w(px(280.))
            .p(px(12.))
            .flex()
            .flex_col()
            .gap(px(10.))
            .child(form::row(&theme, "Name", info.name.clone()))
            .child(form::row(&theme, "Key", info.key.clone()))
            // Only on an error: a standing hint would caption two labelled
            // fields, and any example would name another board.
            .children(info.error.clone().map(|why| form::error(&theme, why)))
            .child(answers(
                &theme,
                "info",
                Some(Box::new(cx.listener(|this, _, window, cx| {
                    this.dismiss_info(&DismissInfo, window, cx)
                }))),
                "Save",
                Box::new(
                    cx.listener(|this, _, window, cx| this.commit_info(&CommitInfo, window, cx)),
                ),
            ))
            // Pressing away discards: nothing is filed until Save. The press is
            // spent on the dismissal and reaches nothing behind the panel — the
            // rule [`popover::dismiss_on_out`] states.
            .on_mouse_down_out(cx.listener(|this, _, window, cx| {
                this.dismiss_info(&DismissInfo, window, cx);
                cx.stop_propagation();
            }))
            .into_any_element();
        Some(popover::anchored_menu_below(
            SharedString::from("board-info"),
            card,
            None,
        ))
    }
}
