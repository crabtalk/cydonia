//! The board pane: lanes of cards, and the one field that writes them.

use crate::{
    model::session::ChatSession,
    view::{
        component::menu::{self, Menu},
        leaf::Pane,
        root::{Cydonia, NewBoard},
        sidebar::Renaming,
    },
};
use artifact::{board::Card, layout::Member};
use bezel::ui::scroll as scrollbars;
use bezel::{
    gpui::{
        self, AnyElement, App, ClipboardItem, Context, Div, DragMoveEvent, Entity, Focusable as _,
        FontWeight, KeyBinding, Pixels, Render, ScrollHandle, SharedString, Stateful, Window,
        actions, div, prelude::*, px,
    },
    motion::Painter,
    theme::{TextStyle, Theme, Typeset},
    ui::{
        icons,
        input::{self, Shape, TextField},
        loaders,
        menu::Item,
        popover,
        scroll::{self, Axes, DriftState, FollowState},
        widgets::Buttons,
    },
};
use markdown::Typography;
use std::{cell::RefCell, collections::HashMap};

actions!(cydonia_board, [CommitCard, DismissCard]);

/// Claimed on top of `TextField`, so `enter` files the card here and stays a
/// newline in every other multi-line field.
const KEY_CONTEXT: &str = "CydoniaCard";

const COLUMN_WIDTH: f32 = 272.;

/// What the board holds itself off the window's edges by, and how far short of
/// the foot a lane's bar stops — the session's bar clears its composer the same
/// way, through [`scroll::Overlay::end_inset`].
const BOARD_INSET: f32 = 16.;

/// What separates two lanes, and the room the lane's scrollbar sits in. Handed
/// to the bar through [`scroll::Overlay::channel`], which centres the thumb in
/// it.
const LANE_CHANNEL: Pixels = px(18.);

/// How much of a card is shown before it is cut off. A card is a card: what
/// does not fit in this much of a lane is read by opening it.
const CARD_MAX_HEIGHT: f32 = 140.;

/// Where the card editor would start scrolling inside itself. Set past any
/// card worth a lane, which is to say never: a scroll box inside a scrolling
/// column is two wheels for one gesture, and a run dragged into the half that
/// is not showing cannot be reached. The lane is the scroller, and the editor
/// grows until the lane has to move — which is where GitHub's boards draw the
/// same line.
///
/// A number rather than no cap, because [`Shape::Grow`] takes one.
const CARD_EDITOR_MAX_ROWS: usize = 512;

/// What the document ladder is brought down to on a card — `Callout` over
/// `Body`, which is the size a card's prose was set at when it was one flat
/// string. Every role moves together, so a `#` heading on a card still reads
/// as a heading, in a lane 272 wide rather than on a page.
const CARD_TEXT_SCALE: f32 = TextStyle::Callout.size() / TextStyle::Body.size();

pub fn bindings() -> Vec<KeyBinding> {
    let ctx = Some(KEY_CONTEXT);
    vec![
        KeyBinding::new("enter", CommitCard, ctx),
        KeyBinding::new("shift-enter", input::InsertNewline, ctx),
        KeyBinding::new("escape", DismissCard, ctx),
    ]
}

/// The board's one text field — whichever card is being written or rewritten.
/// Only ever one is open, and a field per card would mint an entity for every
/// row on the board.
pub fn field(cx: &mut App) -> Entity<TextField> {
    cx.new(|cx| {
        TextField::new(cx)
            .with_shape(Shape::Grow {
                min: 2,
                max: CARD_EDITOR_MAX_ROWS,
            })
            .with_key_context(KEY_CONTEXT)
            .with_placeholder("what needs doing…")
    })
}

/// A card's text, read as the document it is. Somebody writing `- [ ] ship it`
/// on a card meant a box to tick, not three characters of punctuation — and the
/// field that writes the card is one click away, which is where the source
/// belongs.
///
/// The document renderer rather than a pass over the inline marks: a card takes
/// whatever was typed on it, and a list, a fence or a link is no less a card for
/// being one. What does not fit is cut off by [`CARD_MAX_HEIGHT`], the same as a
/// long paragraph.
fn card_body(text: &str, window: &mut Window, cx: &mut App) -> AnyElement {
    markdown::render_with(
        &markdown::parse(text),
        markdown::Editing {
            // A picture in a lane this narrow is a picture. Its alt text spelled
            // out underneath would be most of the card.
            caption: markdown::Caption::Hidden,
            typography: Some(Typography::of(cx).scaled(CARD_TEXT_SCALE)),
            ..Default::default()
        },
        window,
        cx,
    )
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

/// Where a [`Cydonia::landing_mark`] hangs.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Mark {
    /// In the gap over the card the drop would land in front of.
    Above,
    /// Inside the top of a lane's first card. The gap over that one is outside
    /// the pane, and a mark drawn there is clipped away by the very scroller
    /// that makes the lane a lane.
    Top,
    /// In the gap under the last card, which is a drop at the end of the lane.
    Below,
    /// In the flow of an empty lane, which has nothing under it to push down.
    Flow,
}

/// A card in flight, named rather than carried: the board is read afresh
/// wherever the drop lands, and a copy of the card travelling with the pointer
/// would be a second one to keep in step with it.
#[derive(Clone)]
pub struct CardDrag(String);

/// Where the card in the air would land — the lane, and the card it would go
/// in front of, `None` being the end of the lane.
///
/// Held as a card rather than a position for the reason
/// [`artifact::board::Board::move_card_before`] takes one: a position means
/// whatever the lane looked like when it was counted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Landing {
    pub column: String,
    pub before: Option<String>,
}

/// One scroll, one drift and one follow per lane, minted the first time the
/// lane is drawn.
///
/// gpui keys a pane's own scroll state by element id and needs nothing from
/// us, but a drift moves that scroll from outside, and moving it takes a
/// handle the view holds. Kept for as long as the window is open: a lane
/// deleted and remade is a new id, and a handful of dropped handles is cheaper
/// than a sweep that has to know which lanes are still on the board.
#[derive(Default)]
pub struct Lanes(RefCell<HashMap<String, (ScrollHandle, DriftState, FollowState)>>);

impl Lanes {
    fn of(&self, id: &str) -> (ScrollHandle, DriftState, FollowState) {
        self.0
            .borrow_mut()
            .entry(id.to_owned())
            .or_default()
            .clone()
    }

    /// Pin the lane to its end again. A lane remembers being scrolled away
    /// from, and opening an editor at its foot is asking to be taken there.
    fn follow(&self, id: &str) {
        self.of(id).2.follow();
    }
}

/// The card under the pointer while it is being carried.
///
/// An entity because that is what gpui paints a drag with, and the window's
/// top layer is the only place this can be drawn: a ghost inside the lane
/// would be clipped by the very edge it is being carried over.
pub struct HeldCard {
    text: String,
}

impl Render for HeldCard {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = Theme::of(cx).clone();
        div()
            .w(px(COLUMN_WIDTH))
            .max_h(px(CARD_MAX_HEIGHT))
            .overflow_hidden()
            .p(px(10.))
            .rounded(px(Theme::control_radius()))
            .border_1()
            .border_color(theme.accent)
            .bg(theme.surface_raised)
            .child(card_body(&self.text, window, cx))
    }
}

/// Where a card sits, as the lane draws it: which board it is on, which lane,
/// whether it is the first in that lane, and what comes under it.
///
/// One value rather than five arguments — what the lane knows about a card's
/// place travels together.
/// Where a lane sits, as the board draws it: which board it is on, its place
/// among that board's lanes, and the pane drawing it.
struct Lane<'a> {
    project: usize,
    board: usize,
    id: &'a str,
    at: usize,
    lanes: usize,
    on: Option<&'a Member>,
}

struct Slot<'a> {
    project: usize,
    board: usize,
    id: &'a str,
    column: &'a str,
    first: bool,
    next: Option<&'a str>,
}

impl Cydonia {
    // ── mutations ────────────────────────────────────────────────

    pub fn show_pane(&mut self, pane: Pane, cx: &mut Context<Self>) {
        self.commit(cx);
        self.leaf_mut().pane = pane;
        cx.notify();
    }

    /// The menu's New Board. The sidebar's `+` names a project by the heading
    /// it sits under; the menu bar has only the one in front. Both raise the
    /// dialog that names it — see [`Cydonia::ask_new_board`].
    pub(crate) fn new_board_action(
        &mut self,
        _: &NewBoard,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(project) = self.workspace.read(cx).active else {
            return;
        };
        self.ask_new_board(project, window, cx);
    }

    pub(crate) fn open_board(&mut self, project: usize, ix: usize, cx: &mut Context<Self>) {
        self.commit(cx);
        self.workspace
            .update(cx, |workspace, cx| workspace.open_board(project, ix, cx));
        self.leaf_mut().pane = Pane::Board;
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
        self.leaf()
            .card_field
            .update(cx, |field, cx| field.set_content(text, cx));
        // A lane scrolled away from earlier stays where it was left; opening a
        // card at its foot is asking to be taken back there.
        if let Editing::New(column) = &at {
            self.leaf_mut().lanes.follow(column);
        }
        self.leaf_mut().editing = Some(at);
        window.focus(&self.leaf().card_field.read(cx).focus_handle(cx), cx);
        cx.notify();
    }

    /// File whatever is open before leaving it. Every way out of a pane goes
    /// through here.
    ///
    /// An empty card is not a card — committing nothing drops it rather than
    /// leaving a blank on the board.
    pub(crate) fn commit(&mut self, cx: &mut Context<Self>) {
        self.commit_cell(cx);
        self.rest_ribbon(cx);
        let Some(at) = self.leaf_mut().editing.take() else {
            return;
        };
        let text = self.leaf().card_field.read(cx).content().trim().to_owned();
        self.leaf()
            .card_field
            .update(cx, |field, cx| field.clear(cx));
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
        let Some(at) = self.leaf().editing.clone() else {
            return;
        };
        let board = self.workspace.read(cx).active_board();
        let alive = match &at {
            Editing::New(column) => board.is_some_and(|board| board.column(column).is_some()),
            Editing::Card(card) => board.is_some_and(|board| board.card(card).is_some()),
        };
        if !alive {
            self.leaf_mut().editing = None;
            self.leaf()
                .card_field
                .update(cx, |field, cx| field.clear(cx));
        }
    }

    fn commit_card(&mut self, _: &CommitCard, _: &mut Window, cx: &mut Context<Self>) {
        self.commit(cx);
        cx.notify();
    }

    /// Escape abandons the edit — the one way to leave a card as it was.
    fn dismiss_card(&mut self, _: &DismissCard, _: &mut Window, cx: &mut Context<Self>) {
        self.leaf_mut().editing = None;
        self.leaf()
            .card_field
            .update(cx, |field, cx| field.clear(cx));
        cx.notify();
    }

    /// Where the pointer is, as the lanes and cards under it answer in turn.
    ///
    /// gpui runs a frame's listeners outermost first in the capture phase, so
    /// the answers arrive coarsest first: the board clears what the last move
    /// said, the lane the pointer is inside claims the end of itself, and the
    /// card it is over refines that to a place in the lane. Each one only ever
    /// overwrites something vaguer than itself.
    fn aim_card(&mut self, landing: Option<Landing>, cx: &mut Context<Self>) {
        if self.leaf().landing != landing {
            self.leaf_mut().landing = landing;
            cx.notify();
        }
    }

    /// Let the card go, into the lane the release actually landed on — gpui
    /// hit tests a drop, so that much is this frame's answer however stale
    /// anything else is.
    ///
    /// Where in the lane comes from the aim instead, which is a move old: hold
    /// a card at the edge until the board has drifted a lane under it and let
    /// go without moving, and the aim still names the lane that was there.
    /// Aimed at another lane means the end of this one, which is where a lane
    /// nothing was aimed at takes a card anyway.
    fn drop_card(&mut self, card: &str, column: &str, cx: &mut Context<Self>) {
        let before = self
            .leaf_mut()
            .landing
            .take()
            .filter(|landing| landing.column == column)
            .and_then(|landing| landing.before);
        self.commit(cx);
        let (card, column) = (card.to_owned(), column.to_owned());
        self.workspace.update(cx, |workspace, cx| {
            let moved = workspace
                .active_board_mut()
                .is_some_and(|board| board.move_card_before(&card, &column, before.as_deref()));
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

    /// The card's markdown, as it was written. The source and not what the lane
    /// paints: a card is a document, and the text is what somebody would paste
    /// into the next one.
    fn copy_card(&mut self, card: &str, cx: &mut Context<Self>) {
        let Some(text) = self
            .workspace
            .read(cx)
            .active_board()
            .and_then(|board| board.card(card))
            .map(|card| card.text.clone())
        else {
            return;
        };
        cx.write_to_clipboard(ClipboardItem::new_string(text));
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
        let (copied, doomed) = (card.to_owned(), card.to_owned());
        // The card at rest is a rendered document, not a run of text somebody
        // can drag over — so without this there is no way to get a card's words
        // back out of it short of opening the editor and selecting them.
        let rows = vec![
            menu::row(
                Item::action("Copy text").with_icon(icons::text::Copy),
                move |this, _, cx| this.copy_card(&copied, cx),
            ),
            menu::row(
                Item::action("Delete").with_icon(icons::files::Trash),
                move |this, _, cx| this.ask_delete_card(&doomed, cx),
            ),
        ];
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
    pub fn board(
        &self,
        project: usize,
        board_at: usize,
        on: Option<&Member>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(board) = self.workspace.read(cx).board_in(project, board_at) else {
            return div().flex_1().into_any_element();
        };
        // Read out before drawing: each column borrows the board again.
        let ids: Vec<String> = board
            .columns
            .iter()
            .map(|column| column.id.clone())
            .collect();
        let lanes = ids.len();
        let columns: Vec<AnyElement> = ids
            .iter()
            .enumerate()
            .map(|(at, id)| {
                self.column(
                    Lane {
                        project,
                        board: board_at,
                        id,
                        at,
                        lanes,
                        on,
                    },
                    window,
                    cx,
                )
            })
            .collect();
        div()
            .flex_1()
            .min_h_0()
            .relative()
            .on_action(cx.listener(Self::commit_card))
            .on_action(cx.listener(Self::dismiss_card))
            // Outermost, so it runs first: every move starts from nowhere, and
            // the lane and card the pointer is inside put it back. A pointer
            // over no lane at all leaves nothing aimed, which is what makes
            // dragging a card off the board mean nothing.
            .on_drag_move(cx.listener(|this, event: &DragMoveEvent<CardDrag>, _, cx| {
                this.leaf().board_drift.aim(event.event.position);
                this.aim_card(None, cx);
            }))
            // A release no lane took.
            .on_drop(cx.listener(|this, _: &CardDrag, _, cx| this.aim_card(None, cx)))
            .child(
                scroll::pane("board", Axes::Horizontal)
                    .size_full()
                    .flex()
                    .flex_row()
                    .px(px(BOARD_INSET))
                    .pt(px(BOARD_INSET))
                    .track_scroll(&self.leaf_of(on).board_scroll)
                    .children(columns)
                    .child(self.new_column_lane(cx)),
            )
            .child(scrollbars::Overlay::new(
                "board-bar",
                &self.leaf_of(on).board_scroll,
                bezel::gpui::Axis::Horizontal,
            ))
            // A lane off the side of the window is one a drag cannot reach:
            // reaching for it would mean letting go.
            .child(scroll::drift(
                &self.leaf_of(on).board_scroll,
                &self.leaf_of(on).board_drift,
                Axes::Horizontal,
            ))
            .into_any_element()
    }

    fn column(&self, lane: Lane<'_>, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let Lane {
            project,
            board: board_at,
            id,
            at,
            lanes,
            on,
        } = lane;
        let theme = Theme::of(cx).clone();
        let id = id.to_owned();
        let Some((name, cards)) = self
            .workspace
            .read(cx)
            .board_in(project, board_at)
            .and_then(|board| board.column(&id))
            .map(|column| {
                let cards: Vec<String> = column.cards.iter().map(|card| card.id.clone()).collect();
                (column.name.clone(), cards)
            })
        else {
            return div().into_any_element();
        };
        let mut rows: Vec<AnyElement> = cards
            .iter()
            .enumerate()
            .map(|(at, card)| {
                let next = cards.get(at + 1).map(String::as_str);
                self.card(
                    Slot {
                        project,
                        board: board_at,
                        id: card,
                        column: &id,
                        first: at == 0,
                        next,
                    },
                    on,
                    window,
                    cx,
                )
            })
            .collect();
        // An empty lane has no card to hang the mark off, and nothing under it
        // to be pushed down by one drawn in the flow.
        if cards.is_empty() && self.aimed_at(&id, on, cx) {
            rows.push(self.landing_mark(Mark::Flow, cx));
        }
        if matches!(&self.leaf_of(on).editing, Some(Editing::New(at)) if *at == id) {
            rows.push(self.card_editor(on, cx));
        }

        let lane = id.clone();
        let taken = id.clone();
        let composing = matches!(&self.leaf_of(on).editing, Some(Editing::New(at)) if *at == id);
        let (scroll, drift, follow) = self.leaf_of(on).lanes.of(&id);
        let bar_id = format!("lane-bar-{id}");
        div()
            .flex_none()
            .w(px(COLUMN_WIDTH))
            .h_full()
            .flex()
            .flex_col()
            .gap(px(8.))
            // The whole lane, header and all: a card held over the name of a
            // lane is being put in that lane. Coarser than the cards below it
            // and run before them, so whichever one the pointer is actually
            // over has the last word.
            .on_drag_move(cx.listener({
                let drift = drift.clone();
                move |this, event: &DragMoveEvent<CardDrag>, _, cx| {
                    if !event.bounds.contains(&event.event.position) {
                        return;
                    }
                    // Aimed only while the pointer is over this lane, so the
                    // lane it leaves stops rather than drifting on a pointer
                    // that has gone elsewhere.
                    drift.aim(event.event.position);
                    this.aim_card(
                        Some(Landing {
                            column: lane.clone(),
                            before: None,
                        }),
                        cx,
                    );
                }
            }))
            .on_drop(cx.listener(move |this, drag: &CardDrag, _, cx| {
                this.drop_card(&drag.0, &taken, cx);
            }))
            .child(self.column_header(&id, name, cards.len(), at, lanes, cx))
            .child(
                div()
                    .relative()
                    .flex_1()
                    .min_h_0()
                    .child(
                        scroll::pane(SharedString::from(format!("column-{id}")), Axes::Vertical)
                            .size_full()
                            // The space between two lanes, carried by the lane
                            // rather than as a gap on the row: a bar is clipped
                            // to the pane it reports on, so only a lane that
                            // owns the whole channel can put its thumb down the
                            // middle of it.
                            .pr(LANE_CHANNEL)
                            // The foot of the scroll, where `Add a card` sits:
                            // the lane's own gap ends at the last card, and
                            // without this the row lands on the lane's edge.
                            // `../desktop` pads the same place, by enough to
                            // clear the controls bar it floats there.
                            .pb(px(8.))
                            .track_scroll(&scroll)
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
                    // The lane's own half of the gesture: a card held at the
                    // foot of a full lane brings the rest of it up.
                    .child(scroll::drift(&scroll, &drift, Axes::Vertical))
                    // A new card is written at the lane's end, and grows as it
                    // is typed into: the lane stays at that end for as long as
                    // it is left there, and lets go the moment it is scrolled
                    // up — the transcript's rule, and the same element.
                    .children(composing.then(|| scroll::follow(&scroll, &follow)))
                    .child(
                        scrollbars::Overlay::new(bar_id, &scroll, bezel::gpui::Axis::Vertical)
                            .end_inset(px(BOARD_INSET))
                            .channel(LANE_CHANNEL),
                    ),
            )
            .into_any_element()
    }

    /// The lane's name and count, and the `···` that moves or drops it.
    ///
    /// `at` is where the lane sits among `lanes`, which is what decides whether
    /// it can step either way — read here rather than in the menu, which is
    /// built from what the header was drawn with.
    fn column_header(
        &self,
        id: &str,
        name: String,
        count: usize,
        at: usize,
        lanes: usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = Theme::of(cx).clone();
        let row = div()
            .flex_none()
            .pl(px(4.))
            // The channel the lane's bar runs in, which the scroll below pads
            // its content by: the header is a box of its own and outside that
            // scroll, so it reserves the same room or the `···` stands over the
            // bar while the cards under it stop short of one.
            .pr(LANE_CHANNEL)
            .flex()
            .flex_row()
            .items_center()
            .gap(px(6.))
            .text_style(TextStyle::Subheadline);
        if matches!(&self.renaming, Some(Renaming::Column(at)) if at == id) {
            return row.child(self.name_field(cx)).into_any_element();
        }
        let named = id.to_owned();
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
            .child(
                self.menu_button(
                    SharedString::from(format!("column-menu-{id}")),
                    Some("column"),
                    icons::icon(icons::layout::Ellipsis)
                        .size(px(14.))
                        .text_color(theme.text_faint),
                    Menu::Lane(id.to_owned()),
                    cx,
                )
                .children(self.lane_menu(id, count, at, lanes, cx)),
            )
            .into_any_element()
    }

    /// What the `···` does to a lane: which way it moves, and whether it stays.
    ///
    /// A lane at an end is not offered the step it cannot take, and one still
    /// holding cards carries Delete as a row it cannot choose — the refusal is
    /// worth saying, and a button simply withheld says nothing. The words are
    /// the tools' — see `mcp::tools::board`.
    fn lane_menu(
        &self,
        id: &str,
        count: usize,
        at: usize,
        lanes: usize,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if self.menu != Some(Menu::Lane(id.to_owned())) {
            return None;
        }
        let mut rows = Vec::new();
        if at > 0 {
            let moved = id.to_owned();
            rows.push(menu::row(
                Item::action("Move left").with_icon(icons::arrows::ArrowLeft),
                move |this, _, cx| this.shift_column(&moved, -1, cx),
            ));
        }
        if at + 1 < lanes {
            let moved = id.to_owned();
            rows.push(menu::row(
                Item::action("Move right").with_icon(icons::arrows::ArrowRight),
                move |this, _, cx| this.shift_column(&moved, 1, cx),
            ));
        }
        let drop = Item::action("Delete column").with_icon(icons::files::Trash);
        let drop = match count {
            0 => drop,
            _ => drop
                .disabled()
                .with_tooltip("A column is only where work sits — move the cards out first."),
        };
        let dropped = id.to_owned();
        rows.push(menu::row(drop, move |this, _, cx| {
            this.drop_column(&dropped, cx)
        }));
        let card = SharedString::from(format!("lane-menu-{id}"));
        Some(popover::anchored_menu_below(
            card.clone(),
            self.menu_card(card, rows, cx),
            None,
        ))
    }

    /// Step a lane one place, and keep the menu on it: moving twice is two
    /// presses on the same row, not a menu reopened between them.
    fn shift_column(&mut self, id: &str, step: isize, cx: &mut Context<Self>) {
        self.workspace
            .update(cx, |workspace, cx| workspace.move_column(id, step, cx));
        cx.notify();
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
        at: Slot<'_>,
        on: Option<&Member>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Slot {
            project,
            board: board_at,
            id,
            column,
            first,
            next,
        } = at;
        if matches!(&self.leaf_of(on).editing, Some(Editing::Card(at)) if at == id) {
            return self.card_editor(on, cx);
        }
        let theme = Theme::of(cx).clone();
        let painter = Painter::of(cx);
        let Some((card, handle)) = self
            .workspace
            .read(cx)
            .board_in(project, board_at)
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
        // The mark is drawn by the card it names, and by the last card in a
        // lane aimed at its end. Only while something is in the air: what the
        // last drop left is still sitting in `landing`.
        let ahead = cx.has_active_drag()
            && self
                .leaf_of(on)
                .landing
                .as_ref()
                .is_some_and(|at| at.before.as_deref() == Some(id));
        let behind = next.is_none() && self.aimed_at(column, on, cx);
        // The lane's viewport, to clip the aim below with. A hitbox carries
        // its content mask for the hit test but hands `on_drag_move` the raw
        // bounds, so a card scrolled out of its lane still answers for the
        // strip of window its bounds landed on — the lane's own header, most
        // of the time.
        let (viewport, ..) = self.leaf_of(on).lanes.of(column);
        div()
            .id(SharedString::from(format!("card-{id}")))
            .group("card")
            .flex_none()
            .relative()
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
                            .max_h(px(CARD_MAX_HEIGHT))
                            .overflow_hidden()
                            .child(card_body(&text, window, cx)),
                    )
                    // What is done *to* the card. The row underneath carries
                    // the run; where the card sits is the drag.
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
            // Below the drag threshold nothing is picked up, so a press is
            // still the click that opens the card — and gpui drops the click
            // outright once a drag does start, so a card that was carried
            // somewhere does not also open where it landed.
            .on_drag(CardDrag(id.to_owned()), move |_, _, _, cx| {
                let text = text.clone();
                cx.new(|_| HeldCard { text })
            })
            .on_drag_move(cx.listener({
                let (column, card, next) =
                    (column.to_owned(), id.to_owned(), next.map(str::to_owned));
                move |this, event: &DragMoveEvent<CardDrag>, _, cx| {
                    let at = event.event.position;
                    if !event.bounds.contains(&at) || !viewport.bounds().contains(&at) {
                        return;
                    }
                    // Which half of the card the pointer is in says which side
                    // of it the drop goes: in front of this one, or in front
                    // of whatever is under it — and under the last card is the
                    // end of the lane.
                    let before = match at.y < event.bounds.center().y {
                        true => Some(card.clone()),
                        false => next.clone(),
                    };
                    this.aim_card(
                        Some(Landing {
                            column: column.clone(),
                            before,
                        }),
                        cx,
                    );
                }
            }))
            .on_click(cx.listener(move |this, _, window, cx| {
                this.edit(Editing::Card(opened.clone()), window, cx);
            }))
            .children(
                ahead.then(|| self.landing_mark(if first { Mark::Top } else { Mark::Above }, cx)),
            )
            .children(behind.then(|| self.landing_mark(Mark::Below, cx)))
            .into_any_element()
    }

    /// Whether the card in the air would land at the end of this lane.
    fn aimed_at(&self, column: &str, on: Option<&Member>, cx: &App) -> bool {
        cx.has_active_drag()
            && self
                .leaf_of(on)
                .landing
                .as_ref()
                .is_some_and(|at| at.column == column && at.before.is_none())
    }

    /// Where the card would land, drawn in the gap between two cards rather
    /// than in the flow: a mark taking layout would push every card under it
    /// down, and the aim is read off the bounds it just moved — the mark would
    /// chase the pointer it is answering.
    fn landing_mark(&self, at: Mark, cx: &Context<Self>) -> AnyElement {
        let theme = Theme::of(cx).clone();
        let mark = div().h(px(2.)).rounded_full().bg(theme.accent);
        if at == Mark::Flow {
            return mark.flex_none().into_any_element();
        }
        let mark = mark.absolute().left_0().right_0();
        match at {
            Mark::Above => mark.top(px(-5.)),
            Mark::Top => mark.top_0(),
            _ => mark.bottom(px(-5.)),
        }
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

    fn card_editor(&self, on: Option<&Member>, cx: &Context<Self>) -> AnyElement {
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
            .child(self.leaf_of(on).card_field.clone())
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
