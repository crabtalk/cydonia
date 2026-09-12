//! The board pane: lanes of cards, and the one field that writes them.

use crate::{
    model::session::ChatSession,
    view::{
        component::menu::{self, Menu},
        root::{Cydonia, NewBoard, Pane},
        sidebar::Renaming,
    },
};
use artifact::board::Card;
use bezel::{
    gpui::{
        self, AnyElement, App, Context, Div, Entity, Focusable as _, FontWeight, KeyBinding,
        SharedString, Stateful, Window, actions, div, prelude::*, px,
    },
    motion::Painter,
    theme::{TextStyle, Theme, Typeset},
    ui::{
        icons,
        input::{self, Shape, TextField},
        loaders,
        menu::Item,
        popover,
        widgets::Buttons,
    },
};

actions!(cydonia_board, [CommitCard, DismissCard]);

/// Claimed on top of `TextField`, so `enter` files the card here and stays a
/// newline in every other multi-line field.
const KEY_CONTEXT: &str = "CydoniaCard";

const COLUMN_WIDTH: f32 = 272.;

pub fn init(cx: &mut App) {
    let ctx = Some(KEY_CONTEXT);
    cx.bind_keys([
        KeyBinding::new("enter", CommitCard, ctx),
        KeyBinding::new("shift-enter", input::InsertNewline, ctx),
        KeyBinding::new("escape", DismissCard, ctx),
    ]);
}

/// The board's one text field — whichever card is being written or rewritten.
/// Only ever one is open, and a field per card would mint an entity for every
/// row on the board.
pub fn field(cx: &mut App) -> Entity<TextField> {
    cx.new(|cx| {
        TextField::new(cx)
            .with_shape(Shape::Grow { min: 2, max: 8 })
            .with_key_context(KEY_CONTEXT)
            .with_placeholder("what needs doing…")
    })
}

/// What the field is attached to. By id, never by position: a re-read
/// renumbers, and the field would follow the number onto whatever slid under.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Editing {
    /// A card being written, to land at the end of this column.
    New(String),
    /// A card being rewritten.
    Card(String),
}

impl Cydonia {
    // ── mutations ────────────────────────────────────────────────

    pub fn show_pane(&mut self, pane: Pane, cx: &mut Context<Self>) {
        self.commit(cx);
        self.pane = pane;
        cx.notify();
    }

    /// The menu's New Board. The sidebar's `+` names a project by the heading
    /// it sits under; the menu bar has only the one in front.
    pub(crate) fn new_board_action(
        &mut self,
        _: &NewBoard,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(project) = self.workspace.read(cx).active else {
            return;
        };
        self.new_board(project, cx);
    }

    pub(crate) fn new_board(&mut self, project: usize, cx: &mut Context<Self>) {
        self.select_project(project, cx);
        let ix = self
            .workspace
            .update(cx, |workspace, cx| workspace.new_board(cx));
        if let Some(ix) = ix {
            self.open_board(project, ix, cx);
        }
    }

    pub(crate) fn open_board(&mut self, project: usize, ix: usize, cx: &mut Context<Self>) {
        self.commit(cx);
        self.workspace
            .update(cx, |workspace, cx| workspace.open_board(project, ix, cx));
        self.pane = Pane::Board;
        cx.notify();
    }

    /// Point the field at `at`, filing whatever was already open first — so
    /// clicking straight from one card to another never drops an edit.
    fn edit(&mut self, at: Editing, window: &mut Window, cx: &mut Context<Self>) {
        self.commit(cx);
        let text = match &at {
            Editing::New(_) => String::new(),
            Editing::Card(id) => self
                .workspace
                .read(cx)
                .active_board()
                .and_then(|board| board.card(id))
                .map(|card| card.text.clone())
                .unwrap_or_default(),
        };
        self.card_field
            .update(cx, |field, cx| field.set_content(text, cx));
        self.editing = Some(at);
        window.focus(&self.card_field.read(cx).focus_handle(cx), cx);
        cx.notify();
    }

    /// File whatever is open before leaving it. Every way out of a pane goes
    /// through here.
    ///
    /// An empty card is not a card — committing nothing drops it rather than
    /// leaving a blank on the board.
    pub(crate) fn commit(&mut self, cx: &mut Context<Self>) {
        self.commit_cell(cx);
        let Some(at) = self.editing.take() else {
            return;
        };
        let text = self.card_field.read(cx).content().trim().to_owned();
        self.card_field.update(cx, |field, cx| field.clear(cx));
        self.workspace.update(cx, |workspace, cx| {
            let Some(board) = workspace.active_board_mut() else {
                return;
            };
            match at {
                Editing::New(column) => {
                    if !text.is_empty() {
                        board.add_card(&column, text);
                    }
                }
                Editing::Card(id) => {
                    if text.is_empty() {
                        board.remove_card(&id);
                    } else {
                        board.rewrite_card(&id, &text);
                    }
                }
            }
            workspace.save_board();
            cx.notify();
        });
    }

    /// Let go of an edit a re-read made meaningless, and keep one it did not.
    /// Held by id, so a card that merely moved keeps its open field; only one
    /// that has gone leaves the field pointing at nothing.
    pub(crate) fn drop_stale_edit(&mut self, cx: &mut Context<Self>) {
        let Some(at) = self.editing.clone() else {
            return;
        };
        let board = self.workspace.read(cx).active_board();
        let alive = match &at {
            Editing::New(column) => board.is_some_and(|board| board.column(column).is_some()),
            Editing::Card(card) => board.is_some_and(|board| board.card(card).is_some()),
        };
        if !alive {
            self.editing = None;
            self.card_field.update(cx, |field, cx| field.clear(cx));
        }
    }

    fn commit_card(&mut self, _: &CommitCard, _: &mut Window, cx: &mut Context<Self>) {
        self.commit(cx);
        cx.notify();
    }

    /// Escape abandons the edit — the one way to leave a card as it was.
    fn dismiss_card(&mut self, _: &DismissCard, _: &mut Window, cx: &mut Context<Self>) {
        self.editing = None;
        self.card_field.update(cx, |field, cx| field.clear(cx));
        cx.notify();
    }

    /// Carry a card into another lane, its session with it. The neighbour is
    /// named rather than stepped to — the row that drew the arrow knew it.
    fn move_card(&mut self, card: &str, to: &str, cx: &mut Context<Self>) {
        self.commit(cx);
        let (card, to) = (card.to_owned(), to.to_owned());
        self.workspace.update(cx, |workspace, cx| {
            let moved = workspace
                .active_board_mut()
                .is_some_and(|board| board.move_card(&card, &to));
            if moved {
                workspace.save_board();
            }
            cx.notify();
        });
        cx.notify();
    }

    pub(crate) fn delete_card(&mut self, card: &str, cx: &mut Context<Self>) {
        self.commit(cx);
        let card = card.to_owned();
        self.workspace.update(cx, |workspace, cx| {
            let gone = workspace
                .active_board_mut()
                .and_then(|board| board.remove_card(&card))
                .is_some();
            if gone {
                workspace.save_board();
            }
            cx.notify();
        });
        cx.notify();
    }

    /// A lane at the right-hand end, opened straight into its name.
    fn new_column(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.commit(cx);
        let id = self
            .workspace
            .update(cx, |workspace, cx| workspace.new_column(cx));
        if let Some(id) = id {
            self.start_rename(Renaming::Column(id), window, cx);
        }
    }

    /// Drop a lane. Offered only while it is empty — see
    /// [`artifact::board::Board::remove_column`].
    fn drop_column(&mut self, id: &str, cx: &mut Context<Self>) {
        self.commit(cx);
        let id = id.to_owned();
        self.workspace
            .update(cx, |workspace, cx| workspace.remove_column(&id, cx));
        cx.notify();
    }

    /// Hand the card to an agent: a session of its own, opened in the project
    /// with the card's text as its first prompt.
    ///
    /// A session of its own rather than the one in front, so the card's dot
    /// reports its own run — and so dispatching a second card doesn't queue
    /// behind the first. The board stays up: the card goes live where you are
    /// looking, and clicking it is what follows the work into the transcript.
    fn dispatch_card(&mut self, card: &str, cx: &mut Context<Self>) {
        self.commit(cx);
        let card = card.to_owned();
        self.workspace.update(cx, |workspace, cx| {
            let text = workspace
                .active_board()
                .and_then(|board| board.card(&card))
                .map(|card| card.text.clone());
            let (Some(text), Some(entry)) = (text, workspace.preferred_agent()) else {
                return;
            };
            // Nothing to link to is nothing to write: clearing the field on a
            // refused dispatch would take the card's last session off it.
            let Some(record) = workspace
                .new_session(entry, Some(text), cx)
                .and_then(|id| workspace.mint_record(id))
            else {
                return;
            };
            if workspace
                .active_board_mut()
                .is_some_and(|board| board.dispatch_card(&card, record))
            {
                // The link is on the board now, so the board has to be written
                // — it is what the ▶ reads after a quit.
                workspace.save_board();
            }
        });
        cx.notify();
    }

    /// The `···` on a card: what the row of glyphs underneath should not carry,
    /// because it cannot be undone.
    fn card_menu(&self, card: &str, cx: &mut Context<Self>) -> Option<AnyElement> {
        if self.menu.as_ref() != Some(&Menu::Card(card.to_owned())) {
            return None;
        }
        let doomed = card.to_owned();
        let rows = vec![menu::row(
            Item::action("Delete").with_icon(icons::files::Trash),
            move |this, _, cx| this.ask_delete_card(&doomed, cx),
        )];
        let id = SharedString::from(format!("card-menu-card-{card}"));
        Some(popover::anchored_menu_below(
            id.clone(),
            self.menu_card(id, rows, cx),
            None,
        ))
    }

    /// The session a card was dispatched to, while it is still open — a card
    /// whose session has been closed is a card you can run again.
    fn card_session<'a>(&self, card: &Card, cx: &'a App) -> Option<&'a ChatSession> {
        let record = card.session.as_deref()?;
        self.workspace.read(cx).session_by_record(record)
    }

    // ── chrome ───────────────────────────────────────────────────

    /// The lanes. A board opens with none, so the lane that makes one is
    /// always drawn — on an empty board it is the whole pane.
    pub fn board(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(board) = self.workspace.read(cx).active_board() else {
            return div().flex_1().into_any_element();
        };
        // Read out before drawing: each column borrows the board again, and
        // needs to know what is beside it to point an arrow at.
        let ids: Vec<String> = board
            .columns
            .iter()
            .map(|column| column.id.clone())
            .collect();
        let columns: Vec<AnyElement> = (0..ids.len()).map(|ix| self.column(&ids, ix, cx)).collect();
        div()
            .flex_1()
            .min_h_0()
            .on_action(cx.listener(Self::commit_card))
            .on_action(cx.listener(Self::dismiss_card))
            .child(
                div()
                    .id("board")
                    .size_full()
                    .overflow_x_scroll()
                    .flex()
                    .flex_row()
                    .gap(px(10.))
                    .px(px(16.))
                    .py(px(16.))
                    .children(columns)
                    .child(self.new_column_lane(cx)),
            )
            .into_any_element()
    }

    fn column(&self, ids: &[String], ix: usize, cx: &mut Context<Self>) -> AnyElement {
        let theme = Theme::of(cx).clone();
        let id = ids[ix].clone();
        let Some((name, cards)) = self
            .workspace
            .read(cx)
            .active_board()
            .and_then(|board| board.column(&id))
            .map(|column| {
                let cards: Vec<String> = column.cards.iter().map(|card| card.id.clone()).collect();
                (column.name.clone(), cards)
            })
        else {
            return div().into_any_element();
        };
        // Whichever lanes sit either side — where a card's arrows carry it.
        let left = ix.checked_sub(1).map(|n| ids[n].clone());
        let right = ids.get(ix + 1).cloned();
        let mut rows: Vec<AnyElement> = cards
            .iter()
            .map(|card| self.card(card, left.as_deref(), right.as_deref(), cx))
            .collect();
        if matches!(&self.editing, Some(Editing::New(at)) if *at == id) {
            rows.push(self.card_editor(cx));
        }

        div()
            .flex_none()
            .w(px(COLUMN_WIDTH))
            .h_full()
            .flex()
            .flex_col()
            .gap(px(8.))
            .child(self.column_header(&id, name, cards.len(), cx))
            .child(
                div()
                    .id(SharedString::from(format!("column-{id}")))
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .flex()
                    .flex_col()
                    .gap(px(8.))
                    .children(rows)
                    .child(
                        theme
                            .ghost(SharedString::from(format!("add-card-{id}")))
                            .flex_none()
                            .px(px(8.))
                            .py(px(6.))
                            .gap(px(6.))
                            .child(
                                icons::icon(icons::math::Plus)
                                    .size(px(12.))
                                    .text_color(theme.text_faint),
                            )
                            .child(
                                div()
                                    .text_style(TextStyle::Callout)
                                    .text_color(theme.text_faint)
                                    .child("Add a card"),
                            )
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.edit(Editing::New(id.clone()), window, cx);
                            })),
                    ),
            )
            .into_any_element()
    }

    /// The lane's name and count, and — only while it is empty — the way to be
    /// rid of it. See [`artifact::board::Board::remove_column`].
    fn column_header(
        &self,
        id: &str,
        name: String,
        count: usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = Theme::of(cx).clone();
        let row = div()
            .flex_none()
            .px(px(4.))
            .flex()
            .flex_row()
            .items_center()
            .gap(px(6.))
            .text_style(TextStyle::Subheadline);
        if matches!(&self.renaming, Some(Renaming::Column(at)) if at == id) {
            return row.child(self.name_field(cx)).into_any_element();
        }
        let named = id.to_owned();
        let dropped = id.to_owned();
        row.group("column")
            .child(
                div()
                    .id(SharedString::from(format!("column-name-{id}")))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(theme.text_muted)
                    .cursor_pointer()
                    .child(name)
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.start_rename(Renaming::Column(named.clone()), window, cx);
                    })),
            )
            .child(div().text_color(theme.text_faint).child(count.to_string()))
            .child(div().flex_1())
            .children((count == 0).then(|| {
                theme
                    .ghost(SharedString::from(format!("column-delete-{id}")))
                    .invisible()
                    .group_hover("column", |el| el.visible())
                    .p(px(3.))
                    .child(
                        icons::icon(icons::files::Trash)
                            .size(px(12.))
                            .text_color(theme.text_faint),
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.drop_column(&dropped, cx);
                    }))
            }))
            .into_any_element()
    }

    /// The lane that makes a lane, always at the right-hand end.
    fn new_column_lane(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = Theme::of(cx).clone();
        div()
            .flex_none()
            .w(px(COLUMN_WIDTH))
            .h_full()
            .child(
                theme
                    .ghost("add-column")
                    .flex_none()
                    .px(px(8.))
                    .py(px(6.))
                    .gap(px(6.))
                    .child(
                        icons::icon(icons::math::Plus)
                            .size(px(12.))
                            .text_color(theme.text_faint),
                    )
                    .child(
                        div()
                            .text_style(TextStyle::Callout)
                            .text_color(theme.text_faint)
                            .child("Add a column"),
                    )
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.new_column(window, cx);
                    })),
            )
            .into_any_element()
    }

    fn card(
        &self,
        id: &str,
        left: Option<&str>,
        right: Option<&str>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if matches!(&self.editing, Some(Editing::Card(at)) if at == id) {
            return self.card_editor(cx);
        }
        let theme = Theme::of(cx).clone();
        let painter = Painter::of(cx);
        let Some((card, handle)) = self
            .workspace
            .read(cx)
            .active_board()
            .and_then(|board| board.card(id).map(|card| (card, board.handle_of(card))))
        else {
            return div().into_any_element();
        };
        let text = card.text.clone();
        let chat = self.card_session(card, cx);
        let live = chat.map(|chat| chat.id);
        let sessions = self.workspace.read(cx).settings.features.sessions;
        // The same reading as the sidebar's session row: the card and the row are
        // reporting the same process.
        let running = chat.is_some_and(|chat| chat.streaming);
        let orb = running.then(|| {
            loaders::orb(
                loaders::Orb::Cluster,
                SharedString::from(format!("card-orb-{id}")),
                12.,
                &theme,
                painter,
                cx,
            )
            .into_any_element()
        });
        let (opened, run) = (id.to_owned(), id.to_owned());
        div()
            .id(SharedString::from(format!("card-{id}")))
            .group("card")
            .flex_none()
            .p(px(10.))
            .rounded(px(Theme::control_radius()))
            .border_1()
            .border_color(theme.border)
            .bg(theme.surface_raised)
            .cursor_pointer()
            .hover(|el| el.border_color(theme.text_faint))
            .flex()
            .flex_col()
            .gap(px(6.))
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_start()
                    .gap(px(6.))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .max_h(px(140.))
                            .overflow_hidden()
                            .text_style(TextStyle::Callout)
                            .text_color(theme.text)
                            .child(text),
                    )
                    // What is done *to* the card. The row underneath carries
                    // the moves and the run — one press each, all reversible.
                    .child(
                        self.menu_button(
                            SharedString::from(format!("card-menu-{id}")),
                            Some("card"),
                            icons::icon(icons::layout::Ellipsis)
                                .size(px(14.))
                                .text_color(theme.text_faint),
                            Menu::Card(id.to_owned()),
                            cx,
                        )
                        .children(self.card_menu(id, cx)),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(2.))
                    // What to call this card out loud, in the mono face for
                    // the reason the delete dialog sets a path there.
                    .children(handle.map(|handle| {
                        div()
                            .flex_none()
                            .text_style(TextStyle::Caption)
                            .font_family(theme.font_mono.clone())
                            .text_color(theme.text_faint)
                            .child(handle)
                    }))
                    // On show, not behind a hover — a card's run is what you
                    // look at the board to see, and hiding it would mean
                    // hunting for the one that is working.
                    .children(orb)
                    .child(div().flex_1())
                    .child(
                        div()
                            .invisible()
                            .group_hover("card", |el| el.visible())
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap(px(2.))
                            .children(left.map(|to| {
                                let (card, to) = (id.to_owned(), to.to_owned());
                                self.card_action("left", id, icons::arrows::ChevronLeft, cx)
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        cx.stop_propagation();
                                        this.move_card(&card, &to, cx);
                                    }))
                            }))
                            .children(right.map(|to| {
                                let (card, to) = (id.to_owned(), to.to_owned());
                                self.card_action("right", id, icons::arrows::ChevronRight, cx)
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        cx.stop_propagation();
                                        this.move_card(&card, &to, cx);
                                    }))
                            }))
                            // Handing a card to an agent is opening a session,
                            // so the control goes with them: with sessions off
                            // the play would start nothing, and the card is
                            // still a card without it.
                            .children(sessions.then(|| {
                                match live {
                                    Some(session) => self
                                        .card_action("open", id, icons::social::MessageCircle, cx)
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            cx.stop_propagation();
                                            this.select_session(session, cx);
                                            this.show_pane(Pane::Chat, cx);
                                        })),
                                    None => self
                                        .card_action("run", id, icons::multimedia::Play, cx)
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            cx.stop_propagation();
                                            this.dispatch_card(&run, cx);
                                        })),
                                }
                            })),
                    ),
            )
            .on_click(cx.listener(move |this, _, window, cx| {
                this.edit(Editing::Card(opened.clone()), window, cx);
            }))
            .into_any_element()
    }

    /// One glyph on a card's hover row.
    fn card_action(
        &self,
        name: &'static str,
        id: &str,
        glyph: &'static [u8],
        cx: &Context<Self>,
    ) -> Stateful<Div> {
        let theme = Theme::of(cx).clone();
        theme
            .ghost(SharedString::from(format!("card-{name}-{id}")))
            .p(px(3.))
            .child(
                icons::icon(glyph)
                    .size(px(12.))
                    .text_color(theme.text_faint),
            )
    }

    fn card_editor(&self, cx: &Context<Self>) -> AnyElement {
        let theme = Theme::of(cx).clone();
        div()
            .flex_none()
            .p(px(8.))
            .rounded(px(Theme::control_radius()))
            .border_1()
            .border_color(theme.accent)
            .bg(theme.surface_raised)
            .flex()
            .flex_col()
            .gap(px(4.))
            .child(self.card_field.clone())
            .child(
                div()
                    .text_style(TextStyle::Subheadline)
                    .font_family(theme.font_mono.clone())
                    .text_color(theme.text_faint)
                    .child("enter file · esc cancel"),
            )
            .on_mouse_down_out(cx.listener(|this, _, _, cx| this.commit(cx)))
            .into_any_element()
    }
}
