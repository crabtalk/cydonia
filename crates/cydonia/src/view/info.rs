//! A board's identity panel: its name and the key its handles carry, edited in
//! one place because one is derived from the other.

use crate::view::root::Cydonia;
use bezel::{
    gpui::{
        self, AnyElement, App, Context, Entity, Focusable as _, KeyBinding, SharedString, Window,
        actions, div, prelude::*, px,
    },
    theme::{TextStyle, Theme, Typeset},
    ui::{
        input::TextField,
        popover,
        widgets::{ButtonStyle, Buttons},
    },
};

actions!(cydonia_header, [CommitInfo, DismissInfo]);

/// Claimed on the panel's fields, so `enter` files it.
const INFO_CONTEXT: &str = "CydoniaBoardInfo";

/// How wide the labels run, so the two fields start on one edge.
const LABEL_WIDTH: f32 = 34.;

pub fn init(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("enter", CommitInfo, Some(INFO_CONTEXT)),
        KeyBinding::new("escape", DismissInfo, Some(INFO_CONTEXT)),
    ]);
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

/// One of the panel's fields, holding what is there now.
fn seed(content: String, placeholder: &'static str, cx: &mut App) -> Entity<TextField> {
    let field = cx.new(|cx| {
        TextField::new(cx)
            .with_key_context(INFO_CONTEXT)
            .with_placeholder(placeholder)
    });
    field.update(cx, |field, cx| field.set_content(content, cx));
    field
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
        let name = seed(name, "name this board…", cx);
        let key = seed(key, "KEY", cx);
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
            .child(self.info_row("Name", info.name.clone(), cx))
            .child(self.info_row("Key", info.key.clone(), cx))
            // Only on an error: a standing hint would caption two labelled
            // fields, and any example would name another board.
            .children(info.error.clone().map(|why| {
                div()
                    .text_style(TextStyle::Caption)
                    .text_color(theme.danger)
                    .child(why)
            }))
            .child(
                div()
                    .flex()
                    .flex_row()
                    .justify_end()
                    .gap(px(8.))
                    .child(
                        theme
                            .button("Cancel", ButtonStyle::Ghost, None)
                            .id("info-cancel")
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.dismiss_info(&DismissInfo, window, cx)
                            })),
                    )
                    .child(
                        theme
                            .button("Save", ButtonStyle::Prominent, None)
                            .id("info-save")
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.commit_info(&CommitInfo, window, cx)
                            })),
                    ),
            )
            // Pressing away discards: nothing is filed until Save.
            .on_mouse_down_out(
                cx.listener(|this, _, window, cx| this.dismiss_info(&DismissInfo, window, cx)),
            )
            .into_any_element();
        Some(popover::anchored_menu_below(
            SharedString::from("board-info"),
            card,
            None,
        ))
    }

    fn info_row(&self, label: &str, field: Entity<TextField>, cx: &Context<Self>) -> AnyElement {
        let theme = Theme::of(cx).clone();
        div()
            .flex()
            .flex_row()
            .items_center()
            .gap(px(8.))
            .child(
                div()
                    .flex_none()
                    .w(px(LABEL_WIDTH))
                    .text_style(TextStyle::Caption)
                    .text_color(theme.text_muted)
                    .child(label.to_owned()),
            )
            .child(div().flex_1().min_w_0().child(field))
            .into_any_element()
    }
}
