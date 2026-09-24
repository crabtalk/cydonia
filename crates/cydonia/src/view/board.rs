//! The board pane: lanes of cards or a list of them, and the one field that
//! writes them.

use crate::{
    model::{session::ChatSession, workspace::Showing},
    view::{
        component::{
            menu::{self, Menu},
            transcript,
        },
        leaf::Pane,
        root::{Cydonia, NewBoard},
        sidebar::Renaming,
    },
};
use artifact::{
    board::{Card, Status, View},
    space::Member,
};
use bezel::agent::orbs::engine::Frame;
use bezel::ui::scroll as scrollbars;
use bezel::{
    gpui::{
        self, AnyElement, App, ClipboardItem, Context, Div, DragMoveEvent, Entity, Focusable as _,
        FontWeight, KeyBinding, Pixels, Render, ScrollHandle, SharedString, Stateful, Window,
        actions, div, prelude::*, px,
    },
    theme::{TextStyle, Theme, Typeset},
    ui::{
        floating, icons,
        input::{self, Shape, TextField},
        menu::Item,
        popover,
        scroll::{self, Axes, DriftState, FollowState},
        tooltip::Tooltip,
        widgets::Buttons,
    },
};
use editor::{Editor, EditorEvent};
use markdown::Typography;
use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
    rc::Rc,
};

actions!(
    cydonia_board,
    [
        CommitCard,
        DismissCard,
        FindCard,
        DismissFind,
        CloseCardPreview
    ]
);

/// Claimed on top of `TextField`, so `enter` files the card here and stays a
/// newline in every other multi-line field.
const KEY_CONTEXT: &str = "CydoniaCard";

/// The find field's own, so `escape` puts the bar away and stays whatever it is
/// everywhere else.
const FIND_CONTEXT: &str = "CydoniaBoardFind";
const DRAWER_CONTEXT: &str = "CydoniaCardPreview";

const COLUMN_WIDTH: f32 = 272.;

/// Keep one neighbouring lane ready on either side of the viewport.
fn visible_lanes(offset: Pixels, width: Pixels, count: usize) -> std::ops::Range<usize> {
    let left = (-f32::from(offset) - BOARD_INSET).max(0.);
    let right = (left + f32::from(width)).max(0.);
    let start = (left / COLUMN_WIDTH).floor() as usize;
    let end = (right / COLUMN_WIDTH).ceil() as usize;
    start.saturating_sub(1).min(count)..end.saturating_add(1).min(count)
}

/// What the board holds itself off the window's edges by, and how far short of
/// the foot a lane's bar stops — the session's bar clears its composer the same
/// way, through [`scroll::Overlay::end_inset`].
const BOARD_INSET: f32 = 16.;

/// What separates two lanes, and the room the lane's scrollbar sits in. Handed
/// to the bar through [`scroll::Overlay::channel`], which centres the thumb in
/// it.
const LANE_CHANNEL: Pixels = px(18.);

/// Fixed group and row heights keep list virtualization aligned.
const LIST_HEADING_HEIGHT: f32 = 36.;
const LIST_ROW_HEIGHT: f32 = 36.;
/// Fixed-height rows need only two neighbours beyond each viewport edge.
fn visible_list_rows(top: Pixels, height: Pixels, count: usize) -> std::ops::Range<usize> {
    if top + height < px(-2. * LIST_ROW_HEIGHT) {
        return 0..0;
    }
    let start = (f32::from(top).max(0.) / LIST_ROW_HEIGHT).floor() as usize;
    let end = (f32::from(top + height).max(0.) / LIST_ROW_HEIGHT).ceil() as usize;
    start.saturating_sub(2).min(count)..end.saturating_add(2).min(count)
}

const LIST_HANDLE_WIDTH: f32 = 64.;

/// What the list leaves clear at its foot for the pill floating there — see
/// [`Cydonia::view_pill`].
const PILL_CLEARANCE: f32 = 32.;

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
        KeyBinding::new("escape", DismissFind, Some(FIND_CONTEXT)),
        KeyBinding::new("escape", CloseCardPreview, Some(DRAWER_CONTEXT)),
    ]
}

/// The board's find field — one per pane, so two boards side by side are
/// narrowed separately.
pub fn find_field(cx: &mut App) -> Entity<TextField> {
    cx.new(|cx| {
        TextField::new(cx)
            .with_key_context(FIND_CONTEXT)
            .with_placeholder("find a card…")
    })
}

/// Does this card answer the query? Matched against what a card is named by:
/// its handle, which is how `DEV-38` gets referred to in prose, and its text.
fn card_matches(card: &Card, handle: Option<&str>, query: &str) -> bool {
    let query = query.trim().to_lowercase();
    if query.is_empty() {
        return true;
    }
    card.text.to_lowercase().contains(&query)
        || handle.is_some_and(|handle| handle.to_lowercase().contains(&query))
}

/// The board's one text field — whichever card is being written or rewritten.
/// Only ever one is open, and a field per card would mint an entity for every
/// row on the board.
pub fn field(cx: &mut App) -> Entity<TextField> {
    cx.new(|cx| {
        TextField::new(cx)
            .with_frame(false)
            .with_shape(Shape::Grow {
                min: 2,
                max: CARD_EDITOR_MAX_ROWS,
            })
            .with_key_context(KEY_CONTEXT)
            .with_placeholder("what needs doing…")
    })
}

/// The statuses a chip is drawn for, which is the ones with no motion of their
/// own. `busy` is the orb instead — see the `running` a card is built with.
fn resting(status: Option<Status>) -> Option<Status> {
    status.filter(|status| *status != Status::Busy)
}

/// How the work on a card is going, said in a word — see
/// [`artifact::board::Status`]. Written by whoever is doing the work, which is
/// usually an agent through `board_set_card_status`.
fn status_chip(status: Status, theme: &Theme) -> AnyElement {
    let tint = match status {
        // Never drawn — see [`resting`]. Named so the match stays total if the
        // set grows.
        Status::Busy => theme.accent,
        Status::Blocked => theme.danger,
        Status::Done => theme.text_faint,
    };
    div()
        .flex_none()
        .px(px(5.))
        .rounded_full()
        .border_1()
        .border_color(tint)
        .text_style(TextStyle::Caption)
        .text_color(tint)
        .child(status.key())
        .into_any_element()
}

/// What a lane holds, and what the find query leaves of it. The two are the
/// same number on a board nobody is searching.
#[derive(Clone, Copy)]
struct Tally {
    held: usize,
    shown: usize,
}

/// Render Markdown at the lane's text scale. Full reading uses the drawer.
/// `base` is where a card's relative picture paths resolve: its project's
/// `.cydonia` folder.
fn card_body(
    doc: &markdown::Doc,
    base: Option<&std::path::Path>,
    window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    markdown::render_with(
        doc,
        markdown::Editing {
            base,
            // A picture in a lane this narrow is a picture. Its alt text spelled
            // out underneath would be most of the card.
            caption: markdown::Caption::Hidden,
            copy: markdown::CopyButton::Hidden,
            typography: Some(Typography::of(cx).scaled(CARD_TEXT_SCALE)),
            ..Default::default()
        },
        window,
        cx,
    )
}

/// Measure the full rendered body, while the lane only shows its preview.
fn card_preview(
    doc: &markdown::Doc,
    base: Option<&std::path::Path>,
    shortened: bool,
    overflow: Entity<bool>,
    window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    div()
        .max_h(px(CARD_MAX_HEIGHT))
        .overflow_hidden()
        .child(
            div()
                .relative()
                .child(card_body(doc, base, window, cx))
                .child(
                    gpui::canvas(
                        move |bounds, _, cx| {
                            let clipped = shortened || bounds.size.height > px(CARD_MAX_HEIGHT);
                            overflow.update(cx, |value, cx| {
                                if *value != clipped {
                                    *value = clipped;
                                    cx.notify();
                                }
                            });
                        },
                        |_, _, _, _| {},
                    )
                    .absolute()
                    .size_full(),
                ),
        )
        .into_any_element()
}

/// What the field is attached to. By id, never by position: a re-read
/// renumbers, and the field would follow the number onto whatever slid under.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Editing {
    /// A card being written, to land at this end of this column.
    New(Place, String),
    /// A card being rewritten.
    Card(String),
}

/// The rendered card open in this pane, scoped to its board.
pub struct OpenCard {
    board: Member,
    card: String,
    scroll: ScrollHandle,
    focus: gpui::FocusHandle,
    reveal: Rc<Cell<bool>>,
    bounds: Rc<Cell<gpui::Bounds<Pixels>>>,
    adjustments: Rc<RefCell<HashMap<String, LaneAdjustment>>>,
    size: DrawerSize,
    resize_grab: Option<Pixels>,
    pane_bounds: Rc<Cell<gpui::Bounds<Pixels>>>,
}

#[derive(Clone, Copy)]
struct DrawerSize {
    fraction: f32,
    expanded: bool,
}

impl Default for DrawerSize {
    fn default() -> Self {
        Self {
            fraction: 0.5,
            expanded: false,
        }
    }
}

impl DrawerSize {
    fn fraction(&self, pane_height: Pixels) -> f32 {
        if self.expanded {
            return 1.;
        }
        let height = f32::from(pane_height);
        if height <= 0. {
            return self.fraction;
        }
        self.fraction.max((160. / height).min(1.)).min(1.)
    }

    fn resize(&mut self, pointer_y: Pixels, pane: gpui::Bounds<Pixels>) {
        let height = f32::from(pane.size.height);
        if height <= 0. {
            return;
        }
        self.fraction =
            (f32::from(pane.bottom() - pointer_y) / height).clamp((160. / height).min(1.), 1.);
        self.expanded = false;
    }
}

/// The editor over the card open in a pane's drawer, saved as it is typed in.
/// Kept past the drawer only while its last save failed, so closing and
/// switching cards never discard text.
pub struct CardDraft {
    board: Member,
    card: String,
    /// The card's text as last read from the board or written to it — what a
    /// save checks the board against.
    base: String,
    /// The editor's source when `base` was taken. Differs from `base` where
    /// the editor normalises the markdown, which is not an edit.
    shown: String,
    editor: Entity<Editor>,
    error: Option<String>,
    _subscription: gpui::Subscription,
}

struct LaneAdjustment {
    scroll: ScrollHandle,
    before: gpui::Point<Pixels>,
    after: gpui::Point<Pixels>,
}

impl LaneAdjustment {
    fn restore(&self) {
        if self.scroll.offset() == self.after {
            self.scroll.set_offset(self.before);
        }
    }
}

impl Drop for OpenCard {
    fn drop(&mut self) {
        for adjustment in self.adjustments.borrow().values() {
            adjustment.restore();
        }
    }
}

/// Align oversized cards at the top; otherwise move only the hidden edge.
fn reveal_delta(
    top: Pixels,
    bottom: Pixels,
    visible_top: Pixels,
    visible_bottom: Pixels,
) -> Pixels {
    if top < visible_top || bottom - top > visible_bottom - visible_top {
        visible_top - top
    } else if bottom > visible_bottom {
        visible_bottom - bottom
    } else {
        px(0.)
    }
}

/// Which end of a lane a card being written lands at. The field is drawn at
/// that end too, so what is typed is where it will sit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Place {
    Top,
    End,
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
    /// On the foot of the last row of a group in the list, which is a drop at
    /// the end of that group. The list's rows meet with no gap between them, so
    /// a mark hung in one would lie over the row under it.
    Foot,
    /// In the flow of an empty lane, which has nothing under it to push down.
    Flow,
}

/// A card in flight, named rather than carried: the board is read afresh
/// wherever the drop lands, and a copy of the card travelling with the pointer
/// would be a second one to keep in step with it.
///
/// It says which board it left as well as which card it is: a space can have
/// two boards on screen, and the lane a card lands on cannot tell where it came
/// from.
#[derive(Clone)]
pub struct CardDrag {
    card: String,
    project: usize,
    board: usize,
}

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

/// Where one board sits, wherever it is drawn.
///
/// Keyed by board id on the window rather than held by the pane: the same board
/// arranged in a space and opened on its own is one board, and a handle per
/// pane leaves the two disagreeing about where it is scrolled to.
#[derive(Default)]
pub struct Scrolls(RefCell<HashMap<String, Scroll>>);

/// Everything the panes showing one board scroll by. Cheap to clone — every
/// field is a handle onto shared state, which is what makes two panes on one
/// board move together.
#[derive(Clone, Default)]
pub struct Scroll {
    /// The lanes across.
    pub across: ScrollHandle,
    wheel: Rc<RefCell<gpui::OngoingScroll>>,
    /// The list down, which scrolls the other way — see [`artifact::board::View`].
    /// Its own handle rather than the lanes': one offset read along both axes
    /// would land the list wherever the lanes were scrolled to.
    pub down: ScrollHandle,
    /// What carries a held card past the edge of the window — a lane out of
    /// sight is one a drag cannot reach, because reaching for it means letting
    /// go.
    pub drift: DriftState,
    /// The same, per lane — see [`Lanes`].
    pub lanes: Rc<Lanes>,
}

impl Scrolls {
    /// What `board` is scrolled to, minted the first time it is drawn. Kept for
    /// as long as the window is open, on the same terms as [`Lanes`].
    pub fn of(&self, board: &str) -> Scroll {
        self.0
            .borrow_mut()
            .entry(board.to_owned())
            .or_default()
            .clone()
    }
}

/// The buffer each card's orb paints into, by card id — the same shape as
/// [`Lanes`] and kept on the same terms.
///
/// One apiece and never shared: [`bezel::agent::orbs::orb_element`] fills the
/// buffer as the element is built and reads it back at paint, so two orbs on
/// one buffer would both draw whatever the second put there.
#[derive(Default)]
pub struct Marks(RefCell<HashMap<String, Rc<RefCell<Frame>>>>);

/// Parsed source and bounded lane previews, reused across scroll frames.
/// Full documents remain available to the drawer and list.
#[derive(Default)]
pub struct Docs(RefCell<HashMap<String, CachedDoc>>);

struct CachedDoc {
    full: Rc<markdown::Doc>,
    preview: Option<(Rc<markdown::Doc>, bool)>,
    title: Option<SharedString>,
}

impl Docs {
    fn of(&self, text: &str) -> Rc<markdown::Doc> {
        if let Some(cached) = self.0.borrow().get(text) {
            return cached.full.clone();
        }
        self.0
            .borrow_mut()
            .entry(text.to_owned())
            .or_insert_with(|| CachedDoc {
                full: Rc::new(markdown::parse(text)),
                preview: None,
                title: None,
            })
            .full
            .clone()
    }

    fn title(&self, text: &str) -> SharedString {
        self.of(text);
        let mut docs = self.0.borrow_mut();
        let cached = docs.get_mut(text).unwrap();
        cached
            .title
            .get_or_insert_with(|| list_title(&cached.full).into())
            .clone()
    }

    fn preview(&self, text: &str) -> (Rc<markdown::Doc>, bool) {
        if let Some(cached) = self.0.borrow_mut().get_mut(text) {
            return cached
                .preview
                .get_or_insert_with(|| {
                    let (doc, shortened) = preview_doc(&cached.full);
                    (Rc::new(doc), shortened)
                })
                .clone();
        }
        self.of(text);
        self.preview(text)
    }
}

fn list_title(doc: &markdown::Doc) -> String {
    use markdown::BlockKind;
    for block in &doc.blocks {
        let candidate = match &block.kind {
            BlockKind::Image { alt, .. } => {
                if alt.text.trim().is_empty() {
                    "Image".into()
                } else {
                    alt.text.clone()
                }
            }
            BlockKind::Bookmark { url, .. } => url.clone(),
            BlockKind::Table { header, rows, .. } => header
                .iter()
                .chain(rows.iter().flatten())
                .find(|cell| !cell.text.trim().is_empty())
                .map(|cell| cell.text.clone())
                .unwrap_or_else(|| "Table".into()),
            BlockKind::Rule => continue,
            BlockKind::Code { code, .. } => code
                .text
                .lines()
                .find(|line| !line.trim().is_empty())
                .unwrap_or("Code")
                .to_owned(),
            _ => block
                .text_at(markdown::Part::Body)
                .map(|text| text.text.clone())
                .unwrap_or_default(),
        };
        let mut title = String::new();
        let mut length = 0;
        for word in candidate.split_whitespace() {
            if !title.is_empty() {
                title.push(' ');
                length += 1;
            }
            for ch in word.chars() {
                if length >= 512 {
                    title.push('…');
                    return title;
                }
                title.push(ch);
                length += 1;
            }
        }
        if !title.is_empty() {
            return title;
        }
    }
    "Untitled card".into()
}

/// Bound layout and syntax highlighting for the 140px lane preview only.
fn preview_doc(full: &markdown::Doc) -> (markdown::Doc, bool) {
    let mut shortened = full.blocks.len() > 16;
    let mut doc = markdown::Doc {
        blocks: full.blocks.iter().take(16).cloned().collect(),
    };
    for block in &mut doc.blocks {
        if let markdown::BlockKind::Table { rows, .. } = &mut block.kind {
            shortened |= rows.len() > 12;
            rows.truncate(12);
        }
        let code = matches!(block.kind, markdown::BlockKind::Code { .. });
        for part in block.parts() {
            let Some(text) = block.text_at_mut(part) else {
                continue;
            };
            let mut end = text
                .text
                .char_indices()
                .nth(4096)
                .map(|(at, _)| at)
                .unwrap_or(text.text.len());
            if code {
                end = end.min(
                    text.text
                        .match_indices('\n')
                        .nth(16)
                        .map(|(at, _)| at)
                        .unwrap_or(end),
                );
            }
            if end < text.text.len() {
                shortened = true;
                text.text.truncate(end);
                text.marks.retain_mut(|span| {
                    span.range.end = span.range.end.min(end);
                    span.range.start < span.range.end
                });
            }
        }
    }
    (doc, shortened)
}

impl Marks {
    fn of(&self, card: &str) -> Rc<RefCell<Frame>> {
        self.0
            .borrow_mut()
            .entry(card.to_owned())
            .or_default()
            .clone()
    }
}

/// A card's mark, read off the model with the card: the thinking orb the
/// sidebar's session row and the transcript already use while the card is
/// busy, and the linked session's `#number` while it is not.
struct Working {
    busy: bool,
    /// The linked session's project entry number, for the idle mark.
    number: Option<u64>,
    session: Option<(u64, String)>,
    state: bezel::agent::orbs::OrbState,
    since: std::time::Duration,
    frame: Rc<RefCell<Frame>>,
}

/// When this window opened, for the one orb with nothing better to count from
/// — see [`Cydonia::card_working`].
static SINCE: std::sync::LazyLock<std::time::Instant> =
    std::sync::LazyLock::new(std::time::Instant::now);

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
    base: Option<std::path::PathBuf>,
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
            .child(card_body(
                &markdown::parse(&self.text),
                self.base.as_deref(),
                window,
                cx,
            ))
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

    pub(crate) fn open_board(
        &mut self,
        project: usize,
        ix: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.commit(cx);
        let member = self
            .workspace
            .read(cx)
            .member_of(project, Showing::Board(ix));
        if self.enter_member(member, window, cx) {
            return;
        }
        self.workspace
            .update(cx, |workspace, cx| workspace.open_board(project, ix, cx));
        self.leaf_mut().pane = Pane::Board;
        cx.notify();
    }

    /// Lay a board out the other way — the pill at its foot. The board the pane
    /// is showing rather than the one in front: a space can have two on screen.
    fn set_board_view(&mut self, id: &str, view: View, cx: &mut Context<Self>) {
        self.workspace
            .update(cx, |workspace, cx| workspace.set_board_view(id, view, cx));
        cx.notify();
    }

    /// The board a pane is showing, by its file — the one address every write
    /// to a board is made through. Nothing for a pane on anything else.
    pub(crate) fn pane_board(&self, on: Option<&Member>, cx: &App) -> Option<String> {
        self.workspace
            .read(cx)
            .board_of(on)
            .map(|board| board.id.clone())
    }

    /// Point the field of the pane on `on` at `at`, filing whatever was already
    /// open first — so clicking straight from one card to another never drops
    /// an edit.
    ///
    /// The pane is entered on the way, the same as a press anywhere else in
    /// one: the field, what it commits into and the caret all answer for the
    /// focused pane, and a card opened in a pane the window is not on would be
    /// drawn in one board and filed into another.
    fn edit(
        &mut self,
        on: Option<&Member>,
        at: Editing,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.commit(cx);
        if let Some(on) = on {
            self.focus_pane(&on.clone(), window, cx);
        }
        self.drop_drawer(on, cx);
        let board = self.workspace.read(cx).board_of(on);
        let text = match &at {
            Editing::New(..) => String::new(),
            Editing::Card(id) => board
                .and_then(|board| board.card(id))
                .map(|card| card.text.clone())
                .unwrap_or_default(),
        };
        let id = board.map(|board| board.id.clone());
        let leaf = self.leaf_of(on);
        leaf.card_field
            .update(cx, |field, cx| field.set_content(text, cx));
        // A lane scrolled away from earlier stays where it was left; opening a
        // card at its foot is asking to be taken back there.
        if let (Editing::New(Place::End, column), Some(id)) = (&at, &id) {
            self.boards.of(id).lanes.follow(column);
        }
        let field = self.leaf_of(on).card_field.clone();
        self.leaf_of_mut(on).editing = Some(at);
        window.focus(&field.read(cx).focus_handle(cx), cx);
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
        // Every pane with a field open, not only the focused one: the press
        // that leaves a card behind is often a press into another pane, and by
        // the time this runs the focus has already moved there.
        for at in 0..self.leaves.len() {
            self.commit_leaf(at, cx);
        }
    }

    /// File the card one pane has open, into that pane's own board.
    fn commit_leaf(&mut self, leaf: usize, cx: &mut Context<Self>) {
        let Some(at) = self
            .leaves
            .get_mut(leaf)
            .and_then(|leaf| leaf.editing.take())
        else {
            return;
        };
        let (field, on) = {
            let leaf = &self.leaves[leaf];
            (leaf.card_field.clone(), leaf.entry.clone())
        };
        let text = field.read(cx).content().trim().to_owned();
        field.update(cx, |field, cx| field.clear(cx));
        // The board of the pane the field was open in, which is the one it was
        // drawn over — see [`Self::pane_board`].
        let Some(id) = self.pane_board(on.as_ref(), cx) else {
            return;
        };
        self.workspace.update(cx, |workspace, cx| {
            workspace.write_board(&id, |board| match at {
                Editing::New(place, column) => {
                    if !text.is_empty() {
                        match place {
                            Place::Top => board.prepend_card(&column, text),
                            Place::End => board.add_card(&column, text),
                        };
                    }
                }
                Editing::Card(id) => {
                    if text.is_empty() {
                        board.remove_card(&id);
                    } else {
                        board.rewrite_card(&id, &text);
                    }
                }
            });
            cx.notify();
        });
    }

    /// Let go of an edit a re-read made meaningless, and keep one it did not.
    /// Held by id, so a card that merely moved keeps its open field; only one
    /// that has gone leaves the field pointing at nothing.
    pub(crate) fn drop_stale_edit(&mut self, cx: &mut Context<Self>) {
        for leaf in 0..self.leaves.len() {
            let Some(at) = self.leaves[leaf].editing.clone() else {
                continue;
            };
            let on = self.leaves[leaf].entry.clone();
            let board = self.workspace.read(cx).board_of(on.as_ref());
            let alive = match &at {
                Editing::New(_, column) => {
                    board.is_some_and(|board| board.column(column).is_some())
                }
                Editing::Card(card) => board.is_some_and(|board| board.card(card).is_some()),
            };
            if !alive {
                self.leaves[leaf].editing = None;
                self.leaves[leaf]
                    .card_field
                    .update(cx, |field, cx| field.clear(cx));
            }
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
    /// Let a carried card go on `at` — the board under the pointer, which is
    /// not always the board it was picked up from.
    fn drop_card(
        &mut self,
        drag: &CardDrag,
        at: (usize, usize),
        column: &str,
        cx: &mut Context<Self>,
    ) {
        let before = self
            .leaf_mut()
            .landing
            .take()
            .filter(|landing| landing.column == column)
            .and_then(|landing| landing.before);
        self.commit(cx);
        let (card, column) = (drag.card.clone(), column.to_owned());
        let from = (drag.project, drag.board);
        self.workspace.update(cx, |workspace, cx| {
            match from == at {
                true => {
                    workspace.move_card_within(at, &card, &column, before.as_deref());
                }
                // A card that came off another board arrives under a handle of
                // this one's. Where it lands is the lane it was dropped on,
                // and the place within that lane is not carried over: it is a
                // new card here.
                false => {
                    workspace.carry_card(from, at, &card, Some(&column));
                }
            }
            cx.notify();
        });
        cx.notify();
    }

    /// Put a card at the foot of another lane of its board.
    fn move_card_to(&mut self, board: &str, card: &str, column: &str, cx: &mut Context<Self>) {
        self.commit(cx);
        self.workspace.update(cx, |workspace, cx| {
            workspace.write_board(board, |board| board.move_card_before(card, column, None));
            cx.notify();
        });
        cx.notify();
    }

    pub(crate) fn delete_card(&mut self, board: &str, card: &str, cx: &mut Context<Self>) {
        self.commit(cx);
        let (board, card) = (board.to_owned(), card.to_owned());
        self.workspace.update(cx, |workspace, cx| {
            workspace.write_board(&board, |board| board.remove_card(&card));
            cx.notify();
        });
        cx.notify();
    }

    /// A lane at the right-hand end of one board, opened straight into its
    /// name.
    fn new_column(&mut self, board: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.commit(cx);
        let board = board.to_owned();
        let id = self
            .workspace
            .update(cx, |workspace, cx| workspace.new_column(&board, cx));
        if let Some(id) = id {
            self.start_rename(Renaming::Column(board, id), window, cx);
        }
    }

    /// A lane beside the one the menu is on, opened straight into its name —
    /// the same as [`Self::new_column`], which only ever writes at the end.
    fn new_column_beside(
        &mut self,
        board: &str,
        id: &str,
        after: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.commit(cx);
        let (board, id) = (board.to_owned(), id.to_owned());
        let minted = self.workspace.update(cx, |workspace, cx| {
            workspace.new_column_beside(&board, &id, after, cx)
        });
        if let Some(minted) = minted {
            self.start_rename(Renaming::Column(board, minted), window, cx);
        }
    }

    /// Drop a lane. Offered only while it is empty — see
    /// [`artifact::board::Board::remove_column`] — and asked about first, in
    /// [`crate::view::confirm`].
    pub(crate) fn drop_column(&mut self, board: &str, id: &str, cx: &mut Context<Self>) {
        self.commit(cx);
        let (board, id) = (board.to_owned(), id.to_owned());
        self.workspace
            .update(cx, |workspace, cx| workspace.remove_column(&board, &id, cx));
        cx.notify();
    }

    /// The card's markdown, as it was written. The source and not what the lane
    /// paints: a card is a document, and the text is what somebody would paste
    /// into the next one.
    fn copy_card(&mut self, board: &str, card: &str, cx: &mut Context<Self>) {
        let Some(text) = self
            .workspace
            .read(cx)
            .board_at(board)
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
    fn dispatch_card(&mut self, board: &str, card: &str, cx: &mut Context<Self>) {
        self.commit(cx);
        let (board, card) = (board.to_owned(), card.to_owned());
        self.workspace.update(cx, |workspace, cx| {
            let text = workspace
                .board_at(&board)
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
            // The link lands on the board, so the board is written — it is what
            // the ▶ reads after a quit.
            workspace.write_board(&board, |held| held.dispatch_card(&card, record));
        });
        cx.notify();
    }

    /// The `···` on a card: what the row of glyphs underneath should not carry,
    /// because it cannot be undone.
    fn card_menu(
        &self,
        on: &str,
        card: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if self.menu.as_ref() != Some(&Menu::Card(card.to_owned())) {
            return None;
        }
        let (copied, doomed) = (card.to_owned(), card.to_owned());
        let (from, held) = (on.to_owned(), on.to_owned());
        // The card at rest is a rendered document, not a run of text somebody
        // can drag over — so without this there is no way to get a card's words
        // back out of it short of opening the editor and selecting them.
        // Every lane but the one the card is in, in board order.
        let lanes: Vec<(String, String)> = self
            .workspace
            .read(cx)
            .board_at(on)
            .map(|board| {
                let at = board.column_of(card).map(|column| column.id.clone());
                board
                    .columns
                    .iter()
                    .filter(|column| Some(&column.id) != at.as_ref())
                    .map(|column| (column.id.clone(), column.name.clone()))
                    .collect()
            })
            .unwrap_or_default();
        let mut rows = vec![menu::row(
            Item::action("Copy text").with_icon(icons::text::Copy),
            move |this, _, cx| this.copy_card(&from, &copied, cx),
        )];
        if !lanes.is_empty() {
            let moves = lanes
                .into_iter()
                .map(|(column, name)| {
                    let (board, card) = (on.to_owned(), card.to_owned());
                    menu::row(Item::action(name), move |this, _, cx| {
                        this.move_card_to(&board, &card, &column, cx)
                    })
                })
                .collect();
            rows.push(menu::submenu(
                "Move to",
                icons::arrows::ArrowLeftRight,
                moves,
            ));
        }
        rows.push(menu::row(
            Item::action("Delete").with_icon(icons::files::Trash),
            move |this, _, cx| this.ask_delete_card(&held, &doomed, cx),
        ));
        let id = SharedString::from(format!("card-menu-card-{card}"));
        Some(popover::anchored_menu_below(
            id.clone(),
            self.menu_card(id, rows, window, cx),
            None,
        ))
    }

    /// The session a card was dispatched to, while it is still open — a card
    /// whose session has been closed is a card you can run again.
    fn card_session<'a>(&self, card: &Card, cx: &'a App) -> Option<&'a ChatSession> {
        let record = card.session.as_deref()?;
        self.workspace.read(cx).session_by_record(record)
    }

    /// A card's mark: the orb while it is busy, the linked session's `#number`
    /// while it is not, and nothing when no session is linked. A session stays
    /// linked after the tag is taken off, and one session tags many cards.
    ///
    /// Read out of the model before the card is built — the sidebar's
    /// [`SessionRow`] rule, and for the same reason: the orb leases the frame
    /// clock, which wants the app mutably.
    fn card_working(&self, card: &Card, chat: Option<&ChatSession>) -> Option<Working> {
        let session = chat.map(|chat| (chat.id, chat.label()));
        let number = chat.and_then(|chat| chat.number);
        if card.status != Some(Status::Busy) {
            return chat.map(|chat| Working {
                busy: false,
                number,
                session,
                state: transcript::orb_for(&card.text),
                since: SINCE.elapsed(),
                frame: chat.transcript.mark.clone(),
            });
        }
        match chat.filter(|chat| chat.streaming) {
            Some(chat) => Some(Working {
                busy: true,
                number,
                session,
                state: transcript::orb_of(chat),
                since: chat.elapsed().unwrap_or_default(),
                frame: chat.transcript.mark.clone(),
            }),
            // Busy without a streaming session uses the board’s animation clock.
            None => Some(Working {
                busy: true,
                number,
                session,
                state: transcript::orb_for(&card.text),
                since: SINCE.elapsed(),
                frame: self.card_marks.of(&card.id),
            }),
        }
    }

    fn card_orb(&self, at: Working, cx: &mut Context<Self>) -> AnyElement {
        let theme = Theme::of(cx).clone();
        let orb = match at.busy {
            true => transcript::orb(at.state, at.since, &at.frame, cx),
            false => div()
                .text_style(TextStyle::Caption)
                .font_family(theme.font_mono.clone())
                .text_color(theme.text_faint)
                .child(match at.number {
                    Some(number) => format!("#{number}"),
                    None => "#".into(),
                })
                .into_any_element(),
        };
        let session = at
            .session
            .filter(|_| self.workspace.read(cx).settings.features.sessions);
        div()
            .id("card-busy-orb")
            .debug_selector(|| "card-busy-orb".into())
            .flex_none()
            .child(orb)
            .when_some(session, |el, (session, title)| {
                el.cursor_pointer()
                    .tooltip(move |window, cx| Tooltip::text(title.clone(), window, cx))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        cx.stop_propagation();
                        this.select_session(session, window, cx);
                        this.show_pane(Pane::Chat, cx);
                    }))
            })
            .into_any_element()
    }

    /// Where the board at `board_at` sits — see [`Scrolls`].
    fn scrolls(&self, project: usize, board_at: usize, cx: &App) -> Scroll {
        let id = self
            .workspace
            .read(cx)
            .board_in(project, board_at)
            .map(|board| board.id.clone())
            .unwrap_or_default();
        self.boards.of(&id)
    }

    // ── finding ──────────────────────────────────────────────────

    /// What the board is narrowed by right now, and the empty string when it is
    /// not narrowed at all. Empty while the bar is down whatever the field
    /// still holds, so a query never outlives the thing on screen saying so.
    fn board_query(&self, on: Option<&Member>, cx: &App) -> String {
        let leaf = self.leaf_of(on);
        match leaf.finding {
            true => leaf.find_field.read(cx).content().to_string(),
            false => String::new(),
        }
    }

    /// A lane's name, how many cards it holds, and the ids of the ones the query
    /// leaves standing — in the lane's own order.
    fn lane_cards(
        &self,
        project: usize,
        board_at: usize,
        id: &str,
        query: &str,
        cx: &App,
    ) -> Option<(String, usize, Vec<String>)> {
        let board = self.workspace.read(cx).board_in(project, board_at)?;
        let column = board.column(id)?;
        let cards: Vec<String> = column
            .cards
            .iter()
            .filter(|card| {
                query.trim().is_empty()
                    || card_matches(card, board.handle_of(card).as_deref(), query)
            })
            .map(|card| card.id.clone())
            .collect();
        Some((column.name.clone(), column.cards.len(), cards))
    }

    /// Put the find bar up, take the caret back to a field already up, or —
    /// where the caret is in it already — put it away. The chord that raises a
    /// bar is the one a reader reaches for to be rid of it, and the bar is the
    /// query: dismissing it clears what the board is narrowed by.
    pub(crate) fn find_card(&mut self, _: &FindCard, window: &mut Window, cx: &mut Context<Self>) {
        let field = self.leaf().find_field.clone();
        if self.leaf().finding && field.read(cx).focus_handle(cx).is_focused(window) {
            self.dismiss_find(&DismissFind, window, cx);
            return;
        }
        self.leaf_mut().finding = true;
        window.focus(&field.read(cx).focus_handle(cx), cx);
        cx.notify();
    }

    /// Done finding: the field goes and takes its query with it.
    pub(crate) fn dismiss_find(&mut self, _: &DismissFind, _: &mut Window, cx: &mut Context<Self>) {
        let field = self.leaf().find_field.clone();
        self.leaf_mut().finding = false;
        field.update(cx, |field, cx| field.clear(cx));
        cx.notify();
    }

    /// The bar at the board's top right. Up exactly while the query is —
    /// [`Leaf::finding`] carries no second state for a bar the reader may put
    /// away: a filter with nothing on screen to explain it is a board that has
    /// quietly lost cards.
    fn find_bar(&self, on: Option<&Member>, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.leaf_of(on).finding || cx.has_active_drag() {
            return None;
        }
        let theme = Theme::of(cx).clone();
        Some(
            div()
                .absolute()
                .top(px(BOARD_INSET))
                .right(px(BOARD_INSET))
                .w(px(COLUMN_WIDTH))
                .flex()
                .flex_row()
                .items_center()
                .gap(px(6.))
                .px(px(8.))
                .py(px(2.))
                .rounded_full()
                .border_1()
                .border_color(theme.border)
                .bg(theme.surface_raised)
                .child(
                    icons::icon(icons::text::Search)
                        .size(px(14.))
                        .text_color(theme.text_faint),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .child(self.leaf_of(on).find_field.clone()),
                )
                .child(
                    theme
                        .ghost("board-find-close")
                        .p(px(4.))
                        .rounded_full()
                        .child(
                            icons::icon(icons::notifications::X)
                                .size(px(12.))
                                .text_color(theme.text_faint),
                        )
                        .tooltip(|window, cx| Tooltip::text("Stop finding", window, cx))
                        .on_click(cx.listener(|this, _, window, cx| {
                            cx.stop_propagation();
                            this.dismiss_find(&DismissFind, window, cx);
                        })),
                )
                .into_any_element(),
        )
    }

    // ── chrome ───────────────────────────────────────────────────

    fn open_card(
        &mut self,
        on: Option<&Member>,
        board: Member,
        card: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.commit(cx);
        if let Some(on) = on {
            self.focus_pane(on, window, cx);
        }
        let leaf = self.leaf_of_mut(on);
        if leaf
            .open_card
            .as_ref()
            .is_some_and(|opened| opened.board == board && opened.card == card)
        {
            return self.close_card_preview(on, window, cx);
        }
        if let Some(opened) = &self.leaf_of(on).open_card
            && (opened.board != board || opened.card != card)
        {
            let (was_board, was_card) = (opened.board.clone(), opened.card.clone());
            self.settle_card_draft(on, &was_board, &was_card, cx);
        }
        let leaf = self.leaf_of_mut(on);
        if let Some(opened) = &mut leaf.open_card
            && opened.board == board
        {
            if opened.card != card {
                opened.card = card;
                opened.scroll = ScrollHandle::new();
            }
            opened.reveal.set(true);
        } else {
            leaf.open_card = Some(OpenCard {
                board,
                card,
                scroll: ScrollHandle::new(),
                focus: cx.focus_handle(),
                reveal: Rc::new(Cell::new(true)),
                bounds: Default::default(),
                adjustments: Default::default(),
                size: DrawerSize::default(),
                resize_grab: None,
                pane_bounds: Default::default(),
            });
        }
        let focus = leaf.open_card.as_ref().unwrap().focus.clone();
        self.ensure_card_draft(on, cx);
        window.focus(&focus, cx);
        cx.notify();
    }

    fn close_card_preview(
        &mut self,
        on: Option<&Member>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.drop_drawer(on, cx);
        window.focus(&self.leaf_of(on).focus, cx);
        cx.notify();
    }

    /// Put the drawer away, saving the card it had open.
    fn drop_drawer(&mut self, on: Option<&Member>, cx: &mut Context<Self>) {
        if let Some(opened) = self.leaf_of_mut(on).open_card.take() {
            self.settle_card_draft(on, &opened.board, &opened.card, cx);
        }
    }

    fn drawer_for(
        &self,
        project: usize,
        board_at: usize,
        on: Option<&Member>,
        cx: &App,
    ) -> Option<&OpenCard> {
        let opened = self.leaf_of(on).open_card.as_ref()?;
        let workspace = self.workspace.read(cx);
        if workspace
            .member_of(project, Showing::Board(board_at))
            .as_ref()
            != Some(&opened.board)
        {
            return None;
        }
        if workspace
            .board_in(project, board_at)?
            .card(&opened.card)
            .is_none()
            && self.draft_for(on).is_none()
        {
            return None;
        }
        Some(opened)
    }

    fn draft_for(&self, on: Option<&Member>) -> Option<&CardDraft> {
        let leaf = self.leaf_of(on);
        let opened = leaf.open_card.as_ref()?;
        leaf.card_drafts
            .iter()
            .find(|draft| draft.board == opened.board && draft.card == opened.card)
    }

    /// Where a card's relative picture paths resolve: its project's
    /// `.cydonia` folder, which is where its pasted pictures are kept.
    fn card_base(&self, project: usize, cx: &App) -> Option<std::path::PathBuf> {
        let open = self.workspace.read(cx).projects.get(project)?;
        Some(artifact::project::fs::Project::new(&open.path).cydonia())
    }

    fn draft_mut(
        &mut self,
        on: Option<&Member>,
        board: &Member,
        card: &str,
    ) -> Option<&mut CardDraft> {
        self.leaf_of_mut(on)
            .card_drafts
            .iter_mut()
            .find(|draft| draft.board == *board && draft.card == card)
    }

    /// Put an editor over the card open in the drawer, unless one is there.
    fn ensure_card_draft(&mut self, on: Option<&Member>, cx: &mut Context<Self>) {
        let Some(opened) = self.leaf_of(on).open_card.as_ref() else {
            return;
        };
        if self.draft_for(on).is_some() {
            return;
        }
        let (board, card) = (opened.board.clone(), opened.card.clone());
        let scroll = opened.scroll.clone();
        let workspace = self.workspace.read(cx);
        let Some(text) = workspace
            .board_of(Some(&board))
            .and_then(|held| held.card(&card))
            .map(|held| held.text.clone())
        else {
            return;
        };
        let text_size = workspace.text_size;
        let base = artifact::project::fs::Project::new(&board.project).cydonia();
        let editor = cx.new(|cx| {
            Editor::new(&text, cx)
                .with_base(base)
                .with_text_size(text_size)
                .with_scroll(scroll)
        });
        crate::model::language::ensure(crate::model::article::fences(editor.read(cx)), cx);
        let shown = editor.read(cx).source();
        let (held_on, held_board, held_card) = (on.cloned(), board.clone(), card.clone());
        let subscription = cx.subscribe(&editor, move |this, editor, event: &EditorEvent, cx| {
            if let EditorEvent::Changed = event {
                crate::model::language::ensure(crate::model::article::fences(editor.read(cx)), cx);
                this.save_card(held_on.as_ref(), &held_board, &held_card, cx);
            }
        });
        self.leaf_of_mut(on).card_drafts.push(CardDraft {
            board,
            card,
            base: text,
            shown,
            editor,
            error: None,
            _subscription: subscription,
        });
    }

    /// Write the card's editor to the board, keeping on the draft why the
    /// board refused it.
    fn save_card(
        &mut self,
        on: Option<&Member>,
        board: &Member,
        card: &str,
        cx: &mut Context<Self>,
    ) {
        let Some(draft) = self.draft_mut(on, board, card) else {
            return;
        };
        let (base, shown, editor) = (
            draft.base.clone(),
            draft.shown.clone(),
            draft.editor.clone(),
        );
        let text = editor.read(cx).source();
        let result = match text == shown {
            true => Ok(()),
            false => self.workspace.update(cx, |workspace, cx| {
                workspace.save_card_draft(board, card, &base, &text, cx)
            }),
        };
        if let Some(draft) = self.draft_mut(on, board, card) {
            match result {
                Ok(()) => {
                    if text != draft.shown {
                        draft.base = text.clone();
                        draft.shown = text;
                    }
                    draft.error = None;
                }
                Err(error) => draft.error = Some(error),
            }
        }
        cx.notify();
    }

    /// Let a card's editor go with its drawer, unless its last save failed.
    fn settle_card_draft(
        &mut self,
        on: Option<&Member>,
        board: &Member,
        card: &str,
        _: &mut Context<Self>,
    ) {
        self.leaf_of_mut(on)
            .card_drafts
            .retain(|draft| draft.board != *board || draft.card != card || draft.error.is_some());
    }

    /// Drop the editor's text and read the card back off the board.
    fn reload_card_draft(&mut self, on: Option<&Member>, cx: &mut Context<Self>) {
        let Some(opened) = self.leaf_of(on).open_card.as_ref() else {
            return;
        };
        let (board, card) = (opened.board.clone(), opened.card.clone());
        self.leaf_of_mut(on)
            .card_drafts
            .retain(|draft| draft.board != board || draft.card != card);
        self.ensure_card_draft(on, cx);
        cx.notify();
    }

    fn drawer_action(
        &self,
        id: &'static str,
        glyph: &'static [u8],
        label: impl Into<SharedString>,
        cx: &Context<Self>,
    ) -> Stateful<Div> {
        let theme = Theme::of(cx);
        theme
            .ghost(id)
            .debug_selector(move || id.to_owned())
            .flex_none()
            .size(px(24.))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(Theme::control_radius()))
            .child(
                icons::icon(glyph)
                    .size(px(14.))
                    .text_color(theme.text_muted),
            )
            .tooltip({
                let label = label.into();
                move |window, cx| Tooltip::text(label.clone(), window, cx)
            })
    }

    fn card_drawer(
        &self,
        project: usize,
        board_at: usize,
        on: Option<&Member>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let opened = self.drawer_for(project, board_at, on, cx)?;
        let board = self.workspace.read(cx).board_in(project, board_at)?;
        let card = board.card(&opened.card).cloned();
        let card = card.as_ref();
        let handle = card
            .and_then(|card| board.handle_of(card))
            .unwrap_or_else(|| "Missing card".into());
        let status = card.and_then(|card| card.status);
        let draft = self.draft_for(on);
        let error = draft.and_then(|draft| draft.error.clone()).or_else(|| {
            card.is_none().then(|| {
                "This card was removed or moved. Copy your text before discarding it.".into()
            })
        });
        // The board moved under an editor with nothing of its own to keep —
        // an agent's rewrite, most often — so the editor takes the board's.
        if let (Some(card), Some(draft)) = (card, draft)
            && card.text != draft.base
            && draft.editor.read(cx).source() == draft.shown
        {
            let reload_on = on.cloned();
            let this = cx.entity();
            cx.defer(move |cx| {
                this.update(cx, |this, cx| {
                    this.reload_card_draft(reload_on.as_ref(), cx)
                })
            });
        }
        let expanded = opened.size.expanded;
        let scroll = opened.scroll.clone();
        let focus = opened.focus.clone();
        let editor = draft.map(|draft| draft.editor.clone());
        let working = card.and_then(|card| self.card_working(card, self.card_session(card, cx)));
        let theme = Theme::of(cx).clone();
        let bounds = opened.bounds.clone();
        let reveal = opened.reveal.clone();
        let resize_on = on.cloned();
        let (close_on, escape_on, focus_on, discard_on, expand_on) = (
            on.cloned(),
            on.cloned(),
            on.cloned(),
            on.cloned(),
            on.cloned(),
        );
        let body = match &editor {
            Some(editor) => div()
                .min_h_full()
                .cursor(gpui::CursorStyle::IBeam)
                // Below and beside the text is still the card: a press there
                // lands the caret, as it does on an article's page.
                .on_mouse_down(gpui::MouseButton::Left, {
                    let editor = editor.clone();
                    move |event, window, cx| {
                        editor.update(cx, |editor, cx| {
                            editor.press(
                                event.position,
                                event.click_count,
                                event.modifiers,
                                window,
                                cx,
                            )
                        })
                    }
                })
                .child(editor.clone())
                .into_any_element(),
            None => markdown::render_with(
                &self
                    .card_docs
                    .of(&card.map(|card| card.text.clone()).unwrap_or_default()),
                markdown::Editing::default(),
                window,
                cx,
            )
            .into_any_element(),
        };
        Some(
            scroll::contain_wheel(floating::layer("card-drawer"), Axes::Both)
                .debug_selector(|| "card-drawer".into())
                .key_context(DRAWER_CONTEXT)
                .track_focus(&opened.focus)
                // Before the editor takes its own press, which the pane's
                // focus would otherwise take back off it.
                .capture_any_mouse_down(cx.listener(move |this, _, window, cx| {
                    if let Some(on) = &focus_on {
                        this.focus_pane(on, window, cx);
                    }
                }))
                .on_mouse_down(gpui::MouseButton::Left, {
                    let editor = editor.clone();
                    move |_, window, cx| {
                        let typing = editor
                            .as_ref()
                            .is_some_and(|editor| editor.focus_handle(cx).is_focused(window));
                        if !typing {
                            window.focus(&focus, cx);
                        }
                        cx.stop_propagation();
                    }
                })
                .on_action(cx.listener(move |this, _: &CloseCardPreview, window, cx| {
                    this.close_card_preview(escape_on.as_ref(), window, cx);
                }))
                .bottom_0()
                .left_0()
                .right_0()
                .h(gpui::relative(
                    opened.size.fraction(opened.pane_bounds.get().size.height),
                ))
                .bg(theme.surface_raised)
                .rounded_t(px(if expanded {
                    0.
                } else {
                    Theme::control_radius()
                }))
                .border_t_1()
                .border_l_1()
                .border_r_1()
                .border_color(theme.border)
                .shadow(vec![gpui::BoxShadow {
                    color: gpui::hsla(0., 0., 0., 0.12),
                    offset: gpui::point(px(0.), px(-4.)),
                    blur_radius: px(16.),
                    spread_radius: px(-4.),
                    inset: false,
                }])
                .text_color(theme.text)
                .flex()
                .flex_col()
                .overflow_hidden()
                .child(
                    div()
                        .id("card-drawer-resize")
                        .debug_selector(|| "card-drawer-resize".into())
                        .group("drawer-resize")
                        .flex_none()
                        .h(px(9.))
                        .w_full()
                        .cursor_row_resize()
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(
                            div()
                                .w(px(32.))
                                .h(px(2.))
                                .rounded_full()
                                .bg(theme.text_faint.opacity(0.4))
                                .group_hover("drawer-resize", |el| el.bg(theme.text_muted)),
                        )
                        .on_mouse_down(
                            gpui::MouseButton::Left,
                            cx.listener(move |this, event: &gpui::MouseDownEvent, _, cx| {
                                if let Some(opened) =
                                    &mut this.leaf_of_mut(resize_on.as_ref()).open_card
                                {
                                    opened.resize_grab =
                                        Some(event.position.y - opened.bounds.get().top());
                                    cx.notify();
                                }
                                cx.stop_propagation();
                            }),
                        ),
                )
                .child(
                    div()
                        .flex_none()
                        .flex()
                        .items_center()
                        .gap(px(6.))
                        .px(px(16.))
                        .pb(px(6.))
                        .flex_wrap()
                        .text_style(TextStyle::Caption)
                        .text_color(theme.text_muted)
                        .child(div().font_family(theme.font_mono.clone()).child(handle))
                        .children(resting(status).map(|status| status_chip(status, &theme)))
                        .children(working.map(|at| self.card_orb(at, cx)))
                        .child(div().flex_1())
                        .when(error.is_some() && editor.is_some(), |el| {
                            el.child(
                                self.drawer_action(
                                    "card-draft-discard",
                                    icons::glyph::Undo2,
                                    "Discard your text and reload the card",
                                    cx,
                                )
                                .on_click(cx.listener(
                                    move |this, _, _, cx| {
                                        cx.stop_propagation();
                                        this.reload_card_draft(discard_on.as_ref(), cx);
                                    },
                                )),
                            )
                        })
                        .child(
                            self.drawer_action(
                                "card-drawer-expand",
                                if expanded {
                                    icons::arrows::Shrink
                                } else {
                                    icons::arrows::Expand
                                },
                                if expanded {
                                    "Restore drawer"
                                } else {
                                    "Expand drawer"
                                },
                                cx,
                            )
                            .on_click(cx.listener(
                                move |this, _, _, cx| {
                                    cx.stop_propagation();
                                    if let Some(opened) =
                                        &mut this.leaf_of_mut(expand_on.as_ref()).open_card
                                    {
                                        opened.size.expanded = !opened.size.expanded;
                                        opened.reveal.set(!opened.size.expanded);
                                    }
                                    cx.notify();
                                },
                            )),
                        )
                        .child(
                            self.drawer_action(
                                "card-drawer-close",
                                icons::notifications::X,
                                "Close",
                                cx,
                            )
                            .on_click(cx.listener(
                                move |this, _, window, cx| {
                                    cx.stop_propagation();
                                    this.close_card_preview(close_on.as_ref(), window, cx);
                                },
                            )),
                        ),
                )
                .children(error.map(|error| {
                    div()
                        .flex_none()
                        .px(px(16.))
                        .pb(px(8.))
                        .text_style(TextStyle::Caption)
                        .text_color(theme.danger)
                        .child(error)
                }))
                .child(
                    div()
                        .flex_1()
                        .min_h_0()
                        .relative()
                        .child(
                            scroll::pane("card-drawer-body", Axes::Vertical)
                                .size_full()
                                .track_scroll(&scroll)
                                .p(px(16.))
                                .child(body),
                        )
                        .child(scrollbars::Overlay::new(
                            "card-drawer-scroll",
                            &scroll,
                            gpui::Axis::Vertical,
                        )),
                )
                .child(
                    gpui::canvas(
                        move |measured, window, _| {
                            let old = bounds.replace(measured);
                            if old != measured {
                                if !expanded {
                                    reveal.set(true);
                                }
                                window.on_next_frame(|window, _| window.refresh());
                            }
                        },
                        |_, _, _, _| {},
                    )
                    .absolute()
                    .size_full(),
                )
                .into_any_element(),
        )
    }

    /// The board, laid out the way the board says — see
    /// [`artifact::board::View`]. Everything either layout shares sits here:
    /// the pane's actions, and the aim a drag leaving every lane clears.
    pub fn board(
        &self,
        project: usize,
        board_at: usize,
        on: Option<&Member>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some((id, view)) = self
            .workspace
            .read(cx)
            .board_in(project, board_at)
            .map(|board| (board.id.clone(), board.view))
        else {
            return div().flex_1().into_any_element();
        };
        let body = match view {
            View::Lanes => self.lanes(project, board_at, on, window, cx),
            View::List => self.list(project, board_at, on, window, cx),
        };
        let close_on = on.cloned();
        div()
            .id("board-surface")
            .flex_1()
            .min_h_0()
            .relative()
            // A click the drawer and the cards did not take closes the drawer.
            .on_click(cx.listener(move |this, _, window, cx| {
                let Some(opened) = this.leaf_of(close_on.as_ref()).open_card.as_ref() else {
                    return;
                };
                // The focus goes back to the board only from the drawer: the
                // click may have put it in a field of its own.
                let inside = opened.focus.contains_focused(window, cx)
                    || this
                        .draft_for(close_on.as_ref())
                        .is_some_and(|draft| draft.editor.focus_handle(cx).is_focused(window));
                this.drop_drawer(close_on.as_ref(), cx);
                if inside {
                    window.focus(&this.leaf_of(close_on.as_ref()).focus, cx);
                }
                cx.notify();
            }))
            .on_action(cx.listener(Self::commit_card))
            .on_action(cx.listener(Self::dismiss_card))
            .on_action(cx.listener(Self::dismiss_find))
            // Outermost, so it runs first: every move starts from nowhere, and
            // the lane and card the pointer is inside put it back. A pointer
            // over no lane at all leaves nothing aimed, which is what makes
            // dragging a card off the board mean nothing.
            //
            // The drift aimed is this pane's, the one drawn below — a space
            // can have two boards up, and the focused one is not always the
            // one being dragged over.
            .on_drag_move(cx.listener({
                let drift = self.boards.of(&id).drift;
                move |this, event: &DragMoveEvent<CardDrag>, _, cx| {
                    drift.aim(event.event.position);
                    this.aim_card(None, cx);
                }
            }))
            // A release no lane took.
            .on_drop(cx.listener(|this, _: &CardDrag, _, cx| this.aim_card(None, cx)))
            .children(self.drawer_for(project, board_at, on, cx).map(|opened| {
                let bounds = opened.pane_bounds.clone();
                gpui::canvas(
                    move |measured, window, _| {
                        if bounds.replace(measured) != measured {
                            window.on_next_frame(|window, _| window.refresh());
                        }
                    },
                    |_, _, _, _| {},
                )
                .absolute()
                .size_full()
            }))
            .child(body)
            .children(self.view_pill(&id, view, cx))
            .children(self.find_bar(on, cx))
            .children(self.card_drawer(project, board_at, on, window, cx))
            .children(
                self.drawer_for(project, board_at, on, cx)
                    .and_then(|opened| {
                        opened.resize_grab.map(|grab| {
                            let move_on = on.cloned();
                            let release_on = on.cloned();
                            let release =
                                move |this: &mut Self,
                                      _: &gpui::MouseUpEvent,
                                      _: &mut Window,
                                      cx: &mut Context<Self>| {
                                    if let Some(opened) =
                                        &mut this.leaf_of_mut(release_on.as_ref()).open_card
                                    {
                                        opened.resize_grab = None;
                                        cx.notify();
                                    }
                                    cx.stop_propagation();
                                };
                            floating::layer("card-drawer-resizing")
                                .inset_0()
                                .cursor_row_resize()
                                .on_mouse_move(cx.listener(
                                    move |this, event: &gpui::MouseMoveEvent, _, cx| {
                                        if let Some(opened) =
                                            &mut this.leaf_of_mut(move_on.as_ref()).open_card
                                        {
                                            if event.pressed_button == Some(gpui::MouseButton::Left)
                                            {
                                                opened.size.resize(
                                                    event.position.y - grab,
                                                    opened.pane_bounds.get(),
                                                );
                                            } else {
                                                opened.resize_grab = None;
                                            }
                                            cx.notify();
                                        }
                                        cx.stop_propagation();
                                    },
                                ))
                                .on_mouse_up(gpui::MouseButton::Left, cx.listener(release.clone()))
                                .on_mouse_up_out(gpui::MouseButton::Left, cx.listener(release))
                        })
                    }),
            )
            .into_any_element()
    }

    /// The pill at the foot of a board: which way it is laid out, and the press
    /// that lays it out the other way.
    ///
    /// In the pane rather than in the band, because a pane of a space has no
    /// band — see [`crate::view::arrangement`]. `../desktop` floats its controls
    /// at the same edge.
    ///
    /// Nothing at all while a card is in the air: the pill stands over the
    /// corner the card would be dropped in, and a drop it swallowed would be a
    /// card put back where it came from.
    fn view_pill(&self, id: &str, view: View, cx: &mut Context<Self>) -> Option<AnyElement> {
        if cx.has_active_drag() {
            return None;
        }
        let theme = Theme::of(cx).clone();
        let mut pill = div()
            .absolute()
            .right(px(BOARD_INSET))
            .bottom(px(BOARD_INSET))
            .flex()
            .flex_row()
            .items_center()
            .gap(px(2.))
            .p(px(2.))
            .rounded_full()
            .border_1()
            .border_color(theme.border)
            .bg(theme.surface_raised);
        for (at, glyph, label) in [
            (View::Lanes, icons::development::SquareKanban, "Lanes"),
            (View::List, icons::layout::LayoutList, "List"),
        ] {
            let held = id.to_owned();
            let on = at == view;
            pill = pill.child(
                theme
                    .ghost(SharedString::from(format!("board-view-{}", at.key())))
                    .p(px(5.))
                    .rounded_full()
                    .when(on, |el| el.bg(theme.element_active))
                    .child(icons::icon(glyph).size(px(14.)).text_color(match on {
                        true => theme.text,
                        false => theme.text_faint,
                    }))
                    .tooltip(move |window, cx| Tooltip::text(label, window, cx))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        // The pill floats over the board, so the card or row
                        // under it answers for the same press — see the run on
                        // a card, which is held off its own card this way.
                        cx.stop_propagation();
                        this.set_board_view(&held, at, cx);
                    })),
            );
        }
        Some(pill.into_any_element())
    }

    /// The lanes across. A board opens with none, so the lane that makes one is
    /// always drawn — on an empty board it is the whole pane.
    fn lanes(
        &self,
        project: usize,
        board_at: usize,
        on: Option<&Member>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let scroll = self.scrolls(project, board_at, cx);
        let Some(board) = self.workspace.read(cx).board_in(project, board_at) else {
            return div().flex_1().into_any_element();
        };
        // Read out before drawing: each column borrows the board again.
        let held = board.id.clone();
        let ids: Vec<String> = board
            .columns
            .iter()
            .map(|column| column.id.clone())
            .collect();
        let lanes = ids.len();
        let editing_lane = match &self.leaf_of(on).editing {
            Some(Editing::New(_, column)) => Some(column.clone()),
            Some(Editing::Card(card)) => board
                .columns
                .iter()
                .find(|column| column.cards.iter().any(|held| &held.id == card))
                .map(|column| column.id.clone()),
            None => None,
        };
        let measured_width = window
            .use_keyed_state(
                SharedString::from(format!("board-lane-width-{held}-{on:?}")),
                cx,
                |_, _| Rc::new(Cell::new(px(0.))),
            )
            .read(cx)
            .clone();
        let width = measured_width.get();
        let width = if width > px(0.) {
            width
        } else {
            window.viewport_size().width
        };
        let visible = visible_lanes(scroll.across.offset().x, width, lanes);
        let columns: Vec<AnyElement> = ids
            .iter()
            .enumerate()
            .map(|(at, id)| {
                if !visible.contains(&at) && editing_lane.as_deref() != Some(id.as_str()) {
                    return div()
                        .flex_none()
                        .w(px(COLUMN_WIDTH))
                        .h_full()
                        .into_any_element();
                }
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
            .size_full()
            .relative()
            .child(
                scroll::pane("board", Axes::Horizontal)
                    .size_full()
                    .flex()
                    .flex_row()
                    .px(px(BOARD_INSET))
                    .pt(px(BOARD_INSET))
                    .track_scroll(&scroll.across)
                    .children(columns)
                    .child(self.new_column_lane(&held, cx)),
            )
            .child(
                scrollbars::Overlay::new(
                    "board-bar",
                    &scroll.across,
                    bezel::gpui::Axis::Horizontal,
                )
                .when(
                    self.drawer_for(project, board_at, on, cx)
                        .is_some_and(|drawer| drawer.resize_grab.is_some()),
                    |bar| bar.visibility(scrollbars::Visibility::Never),
                ),
            )
            // A lane off the side of the window is one a drag cannot reach:
            // reaching for it would mean letting go.
            .child(scroll::drift(
                &scroll.across,
                &scroll.drift,
                Axes::Horizontal,
                scroll::Beyond::Nothing,
            ))
            // Preview code and tables must not take a sideways swipe from the board.
            .child(
                gpui::canvas(
                    move |bounds, window, _| {
                        if measured_width.replace(bounds.size.width) != bounds.size.width {
                            let view = window.current_view();
                            window.on_next_frame(move |_, cx| cx.notify(view));
                        }
                        window.insert_hitbox(bounds, gpui::HitboxBehavior::Normal)
                    },
                    move |_, hitbox, window, _| {
                        let handle = scroll.across.clone();
                        let gesture = scroll.wheel.clone();
                        let view = window.current_view();
                        window.on_mouse_event(
                            move |event: &gpui::ScrollWheelEvent, phase, window, cx| {
                                if phase != gpui::DispatchPhase::Capture
                                    || !hitbox.should_handle_scroll(window)
                                {
                                    return;
                                }
                                let raw = event.delta.pixel_delta(window.line_height());
                                let mut delta = raw;
                                if event.delta.precise() {
                                    gesture.borrow_mut().filter(&mut delta, event.touch_phase);
                                } else if delta.x.abs() <= delta.y.abs() {
                                    return;
                                }
                                let sideways = delta.x.abs() > delta.y.abs();
                                let filtered_out = delta.x == px(0.)
                                    && delta.y == px(0.)
                                    && (raw.x != px(0.) || raw.y != px(0.));
                                if sideways || filtered_out {
                                    let offset = handle.offset();
                                    let next = gpui::point(
                                        (offset.x + delta.x).clamp(-handle.max_offset().x, px(0.)),
                                        offset.y,
                                    );
                                    if next != offset {
                                        handle.set_offset(next);
                                        cx.notify(view);
                                    }
                                    cx.stop_propagation();
                                }
                            },
                        );
                    },
                )
                .absolute()
                .inset_0(),
            )
            .into_any_element()
    }

    // ── the list ─────────────────────────────────────────────────

    /// The list down: every card of the board in one scroller, under the
    /// heading of the lane it sits in.
    ///
    /// The same cards, the same handles and the same drags as the lanes — a
    /// list is where they are drawn, not a second board. What it trades away is
    /// the lanes side by side; what it buys is a card's whole line at the width
    /// of the pane.
    fn list(
        &self,
        project: usize,
        board_at: usize,
        on: Option<&Member>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let scroll = self.scrolls(project, board_at, cx);
        let Some(board) = self.workspace.read(cx).board_in(project, board_at) else {
            return div().flex_1().into_any_element();
        };
        // Read out before drawing: each group borrows the board again.
        let held = board.id.clone();
        let ids: Vec<String> = board
            .columns
            .iter()
            .map(|column| column.id.clone())
            .collect();
        let lanes = ids.len();
        let measured_height = window
            .use_keyed_state(
                SharedString::from(format!("board-list-height-{held}-{on:?}")),
                cx,
                |_, _| Rc::new(Cell::new(px(0.))),
            )
            .read(cx)
            .clone();
        let height = measured_height.get();
        let height = if height > px(0.) {
            height
        } else {
            window.viewport_size().height
        };
        let viewport = (self.leaf_of(on).editing.is_none() && !cx.has_active_drag())
            .then_some((-scroll.down.offset().y, height));
        let mut group_top = px(0.);
        let groups: Vec<AnyElement> = ids
            .iter()
            .enumerate()
            .map(|(at, id)| {
                self.list_group(
                    Lane {
                        project,
                        board: board_at,
                        id,
                        at,
                        lanes,
                        on,
                    },
                    &mut group_top,
                    viewport,
                    window,
                    cx,
                )
            })
            .collect();
        div()
            .size_full()
            .relative()
            .child(
                scroll::pane("board-list", Axes::Vertical)
                    .size_full()
                    .flex()
                    .flex_col()
                    // Room at the foot for the pill, which floats over the full
                    // width of the last row — see [`Self::view_pill`]. The lanes
                    // need none: what the pill covers there is the empty half of
                    // `Add a column`.
                    .pb(px(BOARD_INSET + PILL_CLEARANCE)
                        + self
                            .drawer_for(project, board_at, on, cx)
                            .map(|drawer| drawer.bounds.get().size.height)
                            .unwrap_or_default())
                    .track_scroll(&scroll.down)
                    .children(groups)
                    .child(self.new_column_row(&held, cx)),
            )
            .child(
                scrollbars::Overlay::new(
                    "board-list-bar",
                    &scroll.down,
                    bezel::gpui::Axis::Vertical,
                )
                .when(
                    self.drawer_for(project, board_at, on, cx)
                        .is_some_and(|drawer| drawer.resize_grab.is_some()),
                    |bar| bar.visibility(scrollbars::Visibility::Never),
                ),
            )
            // A group off the foot of the window is one a drag cannot reach:
            // reaching for it would mean letting go.
            .child(scroll::drift(
                &scroll.down,
                &scroll.drift,
                Axes::Vertical,
                scroll::Beyond::Nothing,
            ))
            .child(
                gpui::canvas(
                    move |bounds, window, _| {
                        if measured_height.replace(bounds.size.height) != bounds.size.height {
                            let view = window.current_view();
                            window.on_next_frame(move |_, cx| cx.notify(view));
                        }
                    },
                    |_, _, _, _| {},
                )
                .absolute()
                .inset_0(),
            )
            .into_any_element()
    }

    /// One lane as a group: its heading, its rows, and the row that writes a
    /// card into it.
    fn list_group(
        &self,
        lane: Lane<'_>,
        group_top: &mut Pixels,
        viewport: Option<(Pixels, Pixels)>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
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
        // The pane's own member, carried into every listener below: what a
        // press opens belongs to the pane it was drawn in, not to whichever
        // one the window is on.
        let pane = on.cloned();
        let query = self.board_query(on, cx);
        let Some((name, held, cards)) = self.lane_cards(project, board_at, &id, &query, cx) else {
            return div().into_any_element();
        };
        let Some((board_id, folded)) = self
            .workspace
            .read(cx)
            .board_in(project, board_at)
            .and_then(|board| {
                let column = board.column(&id)?;
                Some((board.id.clone(), column.collapsed))
            })
        else {
            return div().into_any_element();
        };
        // A lane folded shut still opens for the two things that would
        // otherwise happen out of sight: a query narrowing the board, and the
        // field writing a card into this lane.
        let writing = matches!(&self.leaf_of(on).editing, Some(Editing::New(_, at)) if *at == id);
        let folded = folded && query.trim().is_empty() && !writing;
        let count = if folded { 0 } else { cards.len() };
        let visible = viewport
            .map(|(top, height)| {
                visible_list_rows(top - *group_top - px(LIST_HEADING_HEIGHT), height, count)
            })
            .unwrap_or(0..count);
        *group_top += px(LIST_HEADING_HEIGHT
            + count as f32 * LIST_ROW_HEIGHT
            + if !folded && cards.is_empty() && query.trim().is_empty() {
                LIST_ROW_HEIGHT
            } else {
                0.
            });
        let mut rows: Vec<AnyElement> = cards
            .iter()
            .enumerate()
            .skip(visible.start)
            .take(visible.len())
            .map(|(row, card)| {
                let next = cards.get(row + 1).map(String::as_str);
                self.list_row(
                    Slot {
                        project,
                        board: board_at,
                        id: card,
                        column: &id,
                        first: row == 0,
                        next,
                    },
                    on,
                    window,
                    cx,
                )
            })
            .collect();
        if visible.start > 0 {
            rows.insert(
                0,
                div()
                    .flex_none()
                    .h(px(visible.start as f32 * LIST_ROW_HEIGHT))
                    .into_any_element(),
            );
        }
        if visible.end < count {
            rows.push(
                div()
                    .flex_none()
                    .h(px((count - visible.end) as f32 * LIST_ROW_HEIGHT))
                    .into_any_element(),
            );
        }
        // An empty group has no row to hang the mark off, and nothing under it
        // to be pushed down by one drawn in the flow.
        if !folded && cards.is_empty() && self.aimed_at(&id, on, cx) {
            rows.push(self.landing_mark(Mark::Flow, cx));
        }
        // At the end the field is written into is drawn at: what is being
        // typed sits where the card will.
        if let Some(Editing::New(place, at)) = &self.leaf_of(on).editing
            && *at == id
        {
            let editor = div()
                .px(px(BOARD_INSET))
                .py(px(6.))
                .child(self.card_editor(on, cx))
                .into_any_element();
            match place {
                Place::Top => rows.insert(0, editor),
                Place::End => rows.push(editor),
            }
        }
        let lane = id.clone();
        let taken = id.clone();
        let written = id.clone();
        div()
            .flex_none()
            .flex()
            .flex_col()
            // The whole group, heading and all: a card held over the name of a
            // lane is being put in that lane. Coarser than the rows below it
            // and run before them, so whichever one the pointer is actually
            // over has the last word.
            .on_drag_move(cx.listener({
                let drift = self.scrolls(project, board_at, cx).drift;
                move |this, event: &DragMoveEvent<CardDrag>, _, cx| {
                    if !event.bounds.contains(&event.event.position) {
                        return;
                    }
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
                this.drop_card(drag, (project, board_at), &taken, cx);
            }))
            .child(self.list_group_header(
                &id,
                name,
                Tally {
                    held,
                    shown: cards.len(),
                },
                at,
                lanes,
                folded,
                &board_id,
                on,
                window,
                cx,
            ))
            .children(rows)
            // Nothing to write into a narrowed lane: a card that does not
            // answer the query would be filed and vanish in one gesture.
            .children(
                (!folded && cards.is_empty() && query.trim().is_empty()).then(|| {
                    theme
                        .ghost(SharedString::from(format!("list-add-card-{id}")))
                        .flex_none()
                        .h(px(LIST_ROW_HEIGHT))
                        .px(px(BOARD_INSET))
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
                            this.edit(
                                pane.as_ref(),
                                Editing::New(Place::End, written.clone()),
                                window,
                                cx,
                            );
                        }))
                }),
            )
            .into_any_element()
    }

    /// A group's heading: the chevron that folds the lane, its name and count,
    /// and the `···` that moves or drops it — the lane's own header, on a row
    /// the width of the pane.
    ///
    /// `held` is the whole lane and `shown` what the query left of it. The
    /// `···` is built from `held`: Delete is refused on a lane holding cards,
    /// not on one showing them.
    #[allow(clippy::too_many_arguments)]
    fn list_group_header(
        &self,
        id: &str,
        name: String,
        tally: Tally,
        at: usize,
        lanes: usize,
        folded: bool,
        board: &str,
        on: Option<&Member>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Tally { held, shown } = tally;
        let theme = Theme::of(cx).clone();
        let row = div()
            .id(SharedString::from(format!("list-group-header-{id}")))
            .relative()
            .flex_none()
            .h(px(LIST_HEADING_HEIGHT))
            .px(px(BOARD_INSET))
            .flex()
            .flex_row()
            .items_center()
            .gap(px(6.))
            .text_style(TextStyle::Subheadline)
            .when(folded && self.aimed_at(id, on, cx), |row| {
                row.bg(theme.accent.opacity(0.08)).child(
                    div()
                        .debug_selector(|| "list-collapsed-drop-target".into())
                        .absolute()
                        .inset_0()
                        .child(self.landing_mark(Mark::Foot, cx)),
                )
            });
        if matches!(&self.renaming, Some(Renaming::Column(_, at)) if at == id) {
            return row.child(self.name_field(cx)).into_any_element();
        }
        let folding = (board.to_owned(), id.to_owned());
        let add_on = on.cloned();
        let add_column = id.to_owned();
        let can_add = self.board_query(on, cx).trim().is_empty();
        row.group("list-group")
            .child(
                div()
                    .id(SharedString::from(format!("list-group-fold-{id}")))
                    .debug_selector(|| "list-group-toggle".into())
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .cursor_pointer()
                    .child(
                        icons::icon(if folded {
                            icons::arrows::ChevronRight
                        } else {
                            icons::arrows::ChevronDown
                        })
                        .size(px(12.))
                        .text_color(theme.text_faint),
                    )
                    .child(
                        div()
                            .min_w_0()
                            .text_ellipsis()
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(theme.text_muted)
                            .child(name),
                    )
                    .child(
                        div()
                            .flex_none()
                            .text_color(theme.text_faint)
                            .child(if shown == held {
                                held.to_string()
                            } else {
                                format!("{shown}/{held}")
                            }),
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.workspace.update(cx, |workspace, cx| {
                            workspace.toggle_column_collapsed(&folding.0, &folding.1, cx)
                        });
                        cx.notify();
                    })),
            )
            .children(can_add.then(|| {
                self.drawer_action("list-group-add", icons::math::Plus, "Add card", cx)
                    .on_click(cx.listener(move |this, _, window, cx| {
                        cx.stop_propagation();
                        this.edit(
                            add_on.as_ref(),
                            Editing::New(Place::Top, add_column.clone()),
                            window,
                            cx,
                        );
                    }))
            }))
            .child(
                self.menu_button(
                    SharedString::from(format!("list-group-menu-{id}")),
                    Some("list-group"),
                    icons::layout::Ellipsis,
                    Menu::Lane(id.to_owned()),
                    cx,
                )
                .children(self.lane_menu(
                    id,
                    held,
                    at,
                    lanes,
                    View::List,
                    board,
                    on,
                    window,
                    cx,
                )),
            )
            .into_any_element()
    }

    /// A fixed-height title row; the drawer holds the full card.
    fn list_row(
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
            next,
            ..
        } = at;
        if matches!(&self.leaf_of(on).editing, Some(Editing::Card(at)) if at == id) {
            return div()
                .px(px(BOARD_INSET))
                .py(px(6.))
                .child(self.card_editor(on, cx))
                .into_any_element();
        }
        let theme = Theme::of(cx).clone();
        let Some((card, handle)) = self
            .workspace
            .read(cx)
            .board_in(project, board_at)
            .and_then(|board| board.card(id).map(|card| (card, board.handle_of(card))))
        else {
            return div().into_any_element();
        };
        let text = card.text.clone();
        let status = card.status;
        let on_board = self
            .workspace
            .read(cx)
            .board_in(project, board_at)
            .map(|board| board.id.clone())
            .unwrap_or_default();
        let chat = self.card_session(card, cx);
        let live = chat.map(|chat| chat.id);
        let sessions = self.workspace.read(cx).settings.features.sessions;
        let working = self.card_working(card, chat);
        let (opened, run) = (id.to_owned(), id.to_owned());
        let sent = on_board.clone();
        let pane = on.cloned();
        let member = self
            .workspace
            .read(cx)
            .member_of(project, Showing::Board(board_at));
        let selected = self
            .drawer_for(project, board_at, on, cx)
            .is_some_and(|opened| opened.card == id);

        let ahead = cx.has_active_drag()
            && self
                .leaf_of(on)
                .landing
                .as_ref()
                .is_some_and(|at| at.before.as_deref() == Some(id));
        let behind = next.is_none() && self.aimed_at(column, on, cx);
        // The scroller's viewport, to clip the aim below with — a row scrolled
        // out of the pane still answers for the strip of window its bounds
        // landed on. The lanes clip against their lane; here there is one.
        let viewport = self.scrolls(project, board_at, cx).down;
        let reveal = self
            .drawer_for(project, board_at, on, cx)
            .filter(|drawer| {
                selected
                    && !drawer.size.expanded
                    && drawer.reveal.get()
                    && drawer.bounds.get().size.height > px(0.)
            })
            .map(|drawer| {
                let pending = drawer.reveal.clone();
                let drawer_top = drawer.bounds.get().top();
                let adjustments = drawer.adjustments.clone();
                let scroll = viewport.clone();
                let column = "list".to_owned();
                gpui::canvas(
                    move |bounds, window, _| {
                        if !pending.replace(false) {
                            return;
                        }
                        let delta = reveal_delta(
                            bounds.top(),
                            bounds.bottom(),
                            scroll.bounds().top(),
                            drawer_top - px(8.),
                        );
                        if delta == px(0.) {
                            return;
                        }
                        let before = scroll.offset();
                        let after = gpui::point(before.x, (before.y + delta).min(px(0.)));
                        let mut adjustments = adjustments.borrow_mut();
                        let adjustment =
                            adjustments
                                .entry(column.clone())
                                .or_insert_with(|| LaneAdjustment {
                                    scroll: scroll.clone(),
                                    before,
                                    after: before,
                                });
                        if adjustment.after != before {
                            adjustment.before = before;
                        }
                        adjustment.after = after;
                        scroll.set_offset(after);
                        window.on_next_frame(|window, _| window.refresh());
                    },
                    |_, _, _, _| {},
                )
                .absolute()
                .size_full()
            });
        // Drawn only on the row the pointer is over, taking no room elsewhere,
        // and decided here rather than in a hover style: gpui can resolve hover
        // differently in prepaint and paint.
        let actions = selected
            || self.list_hovered.as_deref() == Some(id)
            || self.menu.as_ref() == Some(&Menu::Card(id.to_owned()));
        let hovered_id = id.to_owned();
        div()
            .id(SharedString::from(format!("list-row-{id}")))
            .debug_selector(|| "board-list-row".into())
            .on_hover(cx.listener(move |this, hovered: &bool, _, cx| {
                let now = match hovered {
                    true => Some(hovered_id.clone()),
                    false if this.list_hovered.as_ref() == Some(&hovered_id) => None,
                    false => return,
                };
                if this.list_hovered != now {
                    this.list_hovered = now;
                    cx.notify();
                }
            }))
            .flex_none()
            .relative()
            .h(px(LIST_ROW_HEIGHT))
            .px(px(BOARD_INSET))
            .flex()
            .flex_row()
            .items_center()
            .gap(px(8.))
            .border_b_1()
            .border_color(theme.border.opacity(0.3))
            .when(selected, |el| el.bg(theme.accent.opacity(0.08)))
            .cursor_pointer()
            .hover(|el| {
                el.bg(if selected {
                    theme.accent.opacity(0.12)
                } else {
                    theme.element_hover
                })
            })
            .children(reveal)
            // What to call this card out loud, in the mono face for the reason
            // the delete dialog sets a path there. Ahead of the text and at a
            // width of its own, so the lines under one another start together.
            .child(
                div()
                    .flex_none()
                    .w(px(LIST_HANDLE_WIDTH))
                    .text_style(TextStyle::Caption)
                    .font_family(theme.font_mono.clone())
                    .text_color(theme.text_faint)
                    .child(handle.unwrap_or_default()),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .text_style(TextStyle::Callout)
                    .text_color(theme.text)
                    .text_ellipsis()
                    .child(self.card_docs.title(&text)),
            )
            .children(resting(status).map(|status| status_chip(status, &theme)))
            // On show, not behind a hover — a card's run is what you look at
            // the board to see, and hiding it would mean hunting for the one
            // that is working.
            .children(working.map(|at| self.card_orb(at, cx)))
            .when(actions, |row| {
                row.child(
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap(px(2.))
                        // Handing a card to an agent is refused once the card says
                        // something about itself: a tag is somebody already holding
                        // it, and a second agent at one task is work done twice.
                        // Opening the session it has stays — reading is not doing.
                        .children((sessions && live.is_none() && status.is_none()).then(|| {
                            self.card_action("list-run", id, icons::multimedia::Play, cx)
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    cx.stop_propagation();
                                    this.dispatch_card(&sent, &run, cx);
                                }))
                        })),
                )
                .child(
                    self.menu_button(
                        SharedString::from(format!("list-card-menu-{id}")),
                        None,
                        icons::layout::Ellipsis,
                        Menu::Card(id.to_owned()),
                        cx,
                    )
                    .children(self.card_menu(&on_board, id, window, cx)),
                )
            })
            .on_drag(
                CardDrag {
                    card: id.to_owned(),
                    project,
                    board: board_at,
                },
                {
                    let base = self.card_base(project, cx);
                    move |_, _, _, cx| {
                        let (text, base) = (text.clone(), base.clone());
                        cx.new(|_| HeldCard { text, base })
                    }
                },
            )
            .on_drag_move(cx.listener({
                let (column, card, next) =
                    (column.to_owned(), id.to_owned(), next.map(str::to_owned));
                move |this, event: &DragMoveEvent<CardDrag>, _, cx| {
                    let at = event.event.position;
                    if !event.bounds.contains(&at) || !viewport.bounds().contains(&at) {
                        return;
                    }
                    // Which half of the row the pointer is in says which side
                    // of it the drop goes: in front of this one, or in front of
                    // whatever is under it — and under the last row is the end
                    // of the group.
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
                cx.stop_propagation();
                if let Some(member) = &member {
                    this.open_card(pane.as_ref(), member.clone(), opened.clone(), window, cx);
                }
            }))
            .children(ahead.then(|| self.landing_mark(Mark::Top, cx)))
            .children(behind.then(|| self.landing_mark(Mark::Foot, cx)))
            .into_any_element()
    }

    /// The row that makes a lane, at the foot of the list — the list's answer
    /// to [`Self::new_column_lane`].
    fn new_column_row(&self, on: &str, cx: &mut Context<Self>) -> AnyElement {
        let on = on.to_owned();
        let theme = Theme::of(cx).clone();
        theme
            .ghost("list-add-column")
            .flex_none()
            .px(px(BOARD_INSET))
            .py(px(8.))
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
                this.new_column(&on, window, cx);
            }))
            .into_any_element()
    }

    // ── the lanes ────────────────────────────────────────────────

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
        let pane = on.cloned();
        let on_board = self
            .workspace
            .read(cx)
            .board_in(project, board_at)
            .map(|board| board.id.clone())
            .unwrap_or_default();
        let query = self.board_query(on, cx);
        let Some((name, held, cards)) = self.lane_cards(project, board_at, &id, &query, cx) else {
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
        // At the end the field is written into is drawn at: what is being
        // typed sits where the card will.
        if let Some(Editing::New(place, at)) = &self.leaf_of(on).editing
            && *at == id
        {
            let editor = self.card_editor(on, cx);
            match place {
                Place::Top => rows.insert(0, editor),
                Place::End => rows.push(editor),
            }
        }

        let lane = id.clone();
        let taken = id.clone();
        // Only a card written at the foot pins the lane there — see
        // [`Self::edit`].
        let composing =
            matches!(&self.leaf_of(on).editing, Some(Editing::New(Place::End, at)) if *at == id);
        let (scroll, drift, follow) = self.scrolls(project, board_at, cx).lanes.of(&id);
        let bar_id = format!("lane-bar-{id}");
        let clearance = self
            .drawer_for(project, board_at, on, cx)
            .map(|drawer| drawer.bounds.get().size.height)
            .unwrap_or_default();
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
                this.drop_card(drag, (project, board_at), &taken, cx);
            }))
            .child(self.column_header(
                &id,
                name,
                Tally {
                    held,
                    shown: cards.len(),
                },
                at,
                lanes,
                &on_board,
                on,
                window,
                cx,
            ))
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
                            .pb(px(8.) + clearance)
                            .track_scroll(&scroll)
                            .flex()
                            .flex_col()
                            .gap(px(8.))
                            .children(rows)
                            // Nothing to write into a narrowed lane: a card
                            // that does not answer the query would be filed
                            // and vanish in one gesture.
                            .children(query.trim().is_empty().then(|| {
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
                                        this.edit(
                                            pane.as_ref(),
                                            Editing::New(Place::End, id.clone()),
                                            window,
                                            cx,
                                        );
                                    }))
                            })),
                    )
                    // The lane's own half of the gesture: a card held at the
                    // foot of a full lane brings the rest of it up.
                    .child(scroll::drift(
                        &scroll,
                        &drift,
                        Axes::Vertical,
                        scroll::Beyond::Neighbour,
                    ))
                    // A new card is written at the lane's end, and grows as it
                    // is typed into: the lane stays at that end for as long as
                    // it is left there, and lets go the moment it is scrolled
                    // up — the transcript's rule, and the same element.
                    .children(composing.then(|| scroll::follow(&scroll, &follow)))
                    .child(
                        scrollbars::Overlay::new(bar_id, &scroll, bezel::gpui::Axis::Vertical)
                            .end_inset(px(BOARD_INSET))
                            .channel(LANE_CHANNEL)
                            .when(
                                self.drawer_for(project, board_at, on, cx)
                                    .is_some_and(|drawer| drawer.resize_grab.is_some()),
                                |bar| bar.visibility(scrollbars::Visibility::Never),
                            ),
                    ),
            )
            .into_any_element()
    }

    /// The lane's name and count, and the `···` that moves or drops it.
    ///
    /// `at` is where the lane sits among `lanes`, which is what decides whether
    /// it can step either way — read here rather than in the menu, which is
    /// built from what the header was drawn with.
    ///
    /// `held` is the whole lane and `shown` what the query left of it. The
    /// `···` is built from `held`: Delete is refused on a lane holding cards,
    /// not on one showing them.
    #[allow(clippy::too_many_arguments)]
    fn column_header(
        &self,
        id: &str,
        name: String,
        tally: Tally,
        at: usize,
        lanes: usize,
        board: &str,
        on: Option<&Member>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Tally { held, shown } = tally;
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
        if matches!(&self.renaming, Some(Renaming::Column(_, at)) if at == id) {
            return row.child(self.name_field(cx)).into_any_element();
        }
        let named = id.to_owned();
        let on_board = board.to_owned();
        row.group("column")
            .child(
                div()
                    .id(SharedString::from(format!("column-name-{id}")))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(theme.text_muted)
                    .cursor_pointer()
                    .child(name)
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.start_rename(
                            Renaming::Column(on_board.clone(), named.clone()),
                            window,
                            cx,
                        );
                    })),
            )
            .child(
                div()
                    .text_color(theme.text_faint)
                    .child(match shown == held {
                        true => held.to_string(),
                        false => format!("{shown}/{held}"),
                    }),
            )
            .child(div().flex_1())
            .child(
                self.menu_button(
                    SharedString::from(format!("column-menu-{id}")),
                    Some("column"),
                    icons::layout::Ellipsis,
                    Menu::Lane(id.to_owned()),
                    cx,
                )
                .children(self.lane_menu(
                    id,
                    held,
                    at,
                    lanes,
                    View::Lanes,
                    board,
                    on,
                    window,
                    cx,
                )),
            )
            .into_any_element()
    }

    /// What the `···` does to a lane: which way it moves, and whether it stays.
    ///
    /// A lane at an end is not offered the step it cannot take, and one still
    /// holding cards carries Delete as a row it cannot choose — the refusal is
    /// worth saying, and a button simply withheld says nothing.
    ///
    /// The step is one place along the board's own order of its columns, which
    /// is the order the tools speak in — see `mcp::tools::board`. What `view`
    /// decides is only what to call it: that order runs across the lanes and
    /// down the list, so the same step is left in one and up in the other.
    #[allow(clippy::too_many_arguments)]
    fn lane_menu(
        &self,
        id: &str,
        count: usize,
        at: usize,
        lanes: usize,
        view: View,
        board: &str,
        pane: Option<&Member>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if self.menu != Some(Menu::Lane(id.to_owned())) {
            return None;
        }
        // What each direction is called, and the arrow that stands for it. The
        // step and the new lane are the same two directions, said the way the
        // space reads.
        let (back, on, both) = match view {
            View::Lanes => (
                ("Left", icons::arrows::ArrowLeft),
                ("Right", icons::arrows::ArrowRight),
                icons::arrows::ArrowLeftRight,
            ),
            View::List => (
                ("Above", icons::arrows::ArrowUp),
                ("Below", icons::arrows::ArrowDown),
                icons::arrows::ArrowUpDown,
            ),
        };
        // First, and the only row here that makes something: the `Add a card`
        // at the lane's foot is a long way down a full lane, and what is
        // written from the head of one belongs at the head of it.
        let written = id.to_owned();
        let pane = pane.cloned();
        let rename_pane = pane.clone();
        let on_board = board.to_owned();
        let mut rows = vec![menu::row(
            Item::action("Add card").with_icon(icons::math::Plus),
            move |this, window, cx| {
                this.edit(
                    pane.as_ref(),
                    Editing::New(Place::Top, written.clone()),
                    window,
                    cx,
                )
            },
        )];
        let renamed = id.to_owned();
        let rename_board = board.to_owned();
        rows.push(menu::row(
            Item::action("Rename").with_icon(icons::design::Pencil),
            move |this, window, cx| {
                if let Some(pane) = &rename_pane {
                    this.focus_pane(pane, window, cx);
                }
                this.start_rename(
                    Renaming::Column(rename_board.clone(), renamed.clone()),
                    window,
                    cx,
                );
            },
        ));
        // Then the two that write a lane either side of this one, so a board
        // is not only ever grown at its right-hand end.
        let beside = [(false, back), (true, on)]
            .map(|(after, (label, icon))| {
                let (beside, made_on) = (id.to_owned(), on_board.clone());
                menu::row(
                    Item::action(label).with_icon(icon),
                    move |this, window, cx| {
                        this.new_column_beside(&made_on, &beside, after, window, cx)
                    },
                )
            })
            .into_iter()
            .collect();
        rows.push(menu::submenu("Add column", icons::math::Plus, beside));
        // The step is offered only the way the lane can take it, so a lane at
        // an end carries the one direction and a board of one carries neither.
        let steps: Vec<_> = [(at > 0, -1, back), (at + 1 < lanes, 1, on)]
            .into_iter()
            .filter(|(can, ..)| *can)
            .map(|(_, step, (label, icon))| {
                let (moved, moved_on) = (id.to_owned(), on_board.clone());
                menu::row(Item::action(label).with_icon(icon), move |this, _, cx| {
                    this.shift_column(&moved_on, &moved, step, cx)
                })
            })
            .collect();
        if !steps.is_empty() {
            rows.push(menu::submenu("Move column", both, steps));
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
            this.ask_delete_column(&on_board, &dropped, cx)
        }));
        let card = SharedString::from(format!("lane-menu-{id}"));
        Some(popover::anchored_menu_below(
            card.clone(),
            self.menu_card(card, rows, window, cx),
            None,
        ))
    }

    /// Step a lane one place, and keep the menu on it: moving twice is two
    /// presses on the same row, not a menu reopened between them.
    fn shift_column(&mut self, board: &str, id: &str, step: isize, cx: &mut Context<Self>) {
        self.workspace.update(cx, |workspace, cx| {
            workspace.move_column(board, id, step, cx)
        });
        cx.notify();
    }

    /// The lane that makes a lane, always at the right-hand end.
    fn new_column_lane(&self, on: &str, cx: &mut Context<Self>) -> AnyElement {
        let on = on.to_owned();
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
                        this.new_column(&on, window, cx);
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
        let Some((card, handle)) = self
            .workspace
            .read(cx)
            .board_in(project, board_at)
            .and_then(|board| board.card(id).map(|card| (card, board.handle_of(card))))
        else {
            return div().into_any_element();
        };
        let text = card.text.clone();
        let status = card.status;
        let on_board = self
            .workspace
            .read(cx)
            .board_in(project, board_at)
            .map(|board| board.id.clone())
            .unwrap_or_default();
        let chat = self.card_session(card, cx);
        let live = chat.map(|chat| chat.id);
        let sessions = self.workspace.read(cx).settings.features.sessions;
        let working = self.card_working(card, chat);
        let (opened, run) = (id.to_owned(), id.to_owned());
        let sent = on_board.clone();
        let pane = on.cloned();
        let member = self
            .workspace
            .read(cx)
            .member_of(project, Showing::Board(board_at));
        let selected = self
            .leaf_of(on)
            .open_card
            .as_ref()
            .is_some_and(|opened| Some(&opened.board) == member.as_ref() && opened.card == id);
        let overflow = window.use_keyed_state(
            SharedString::from(format!("card-overflow-{on:?}-{id}")),
            cx,
            |_, _| false,
        );
        let truncated = *overflow.read(cx);
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
        let (viewport, ..) = self.scrolls(project, board_at, cx).lanes.of(column);
        let reveal = self
            .drawer_for(project, board_at, on, cx)
            .filter(|drawer| {
                selected
                    && !drawer.size.expanded
                    && drawer.reveal.get()
                    && drawer.bounds.get().size.height > px(0.)
            })
            .map(|drawer| {
                let pending = drawer.reveal.clone();
                let drawer_top = drawer.bounds.get().top();
                let adjustments = drawer.adjustments.clone();
                let scroll = viewport.clone();
                let column = column.to_owned();
                gpui::canvas(
                    move |bounds, window, _| {
                        if !pending.replace(false) {
                            return;
                        }
                        let delta = reveal_delta(
                            bounds.top(),
                            bounds.bottom(),
                            scroll.bounds().top(),
                            drawer_top - px(8.),
                        );
                        if delta == px(0.) {
                            return;
                        }
                        let before = scroll.offset();
                        let after = gpui::point(before.x, (before.y + delta).min(px(0.)));
                        let mut adjustments = adjustments.borrow_mut();
                        let adjustment =
                            adjustments
                                .entry(column.clone())
                                .or_insert_with(|| LaneAdjustment {
                                    scroll: scroll.clone(),
                                    before,
                                    after: before,
                                });
                        if adjustment.after != before {
                            adjustment.before = before;
                        }
                        adjustment.after = after;
                        scroll.set_offset(after);
                        window.on_next_frame(|window, _| window.refresh());
                    },
                    |_, _, _, _| {},
                )
                .absolute()
                .size_full()
            });
        div()
            .id(SharedString::from(format!("card-{id}")))
            .group("card")
            .flex_none()
            .relative()
            .p(px(10.))
            .rounded(px(Theme::control_radius()))
            .border_1()
            .border_color(if selected {
                theme.accent
            } else {
                gpui::hsla(0., 0., 0., 0.)
            })
            .bg(theme.surface_raised)
            .cursor_pointer()
            .hover(|el| el.bg(theme.surface_raised_hover))
            .flex()
            .flex_col()
            .gap(px(6.))
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_start()
                    .gap(px(6.))
                    .child(div().flex_1().min_w_0().child({
                        let (doc, shortened) = self.card_docs.preview(&text);
                        let base = self.card_base(project, cx);
                        card_preview(&doc, base.as_deref(), shortened, overflow, window, cx)
                    }))
                    // What is done *to* the card. The row underneath carries
                    // the run; where the card sits is the drag.
                    .child(
                        self.menu_button(
                            SharedString::from(format!("card-menu-{id}")),
                            Some("card"),
                            icons::layout::Ellipsis,
                            Menu::Card(id.to_owned()),
                            cx,
                        )
                        .children(self.card_menu(&on_board, id, window, cx)),
                    ),
            )
            .when(truncated, |el| {
                el.child(
                    div()
                        .text_style(TextStyle::Caption)
                        .text_color(theme.text_faint)
                        .child("…"),
                )
            })
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
                    .children(resting(status).map(|status| status_chip(status, &theme)))
                    .child(div().flex_1())
                    // Where ▶ stands, because it is what ▶ becomes: a card is
                    // either one you can start or one that is running, and the
                    // two belong in one slot. On show rather than behind the
                    // hover the actions sit behind — a card's run is what you
                    // look at the board to see.
                    .children(working.map(|at| self.card_orb(at, cx)))
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
                            // Handing a card to an agent is refused once the card
                            // says something about itself: a tag is somebody
                            // already holding it, and a second agent at one task is
                            // the work done twice. Opening the session it has
                            // stays — reading is not doing.
                            .children((sessions && live.is_none() && status.is_none()).then(
                                || {
                                    self.card_action("run", id, icons::multimedia::Play, cx)
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            cx.stop_propagation();
                                            this.dispatch_card(&sent, &run, cx);
                                        }))
                                },
                            )),
                    ),
            )
            // Below the drag threshold nothing is picked up, so a press is
            // still the click that opens the card — and gpui drops the click
            // outright once a drag does start, so a card that was carried
            // somewhere does not also open where it landed.
            .on_drag(
                CardDrag {
                    card: id.to_owned(),
                    project,
                    board: board_at,
                },
                {
                    let base = self.card_base(project, cx);
                    move |_, _, _, cx| {
                        let (text, base) = (text.clone(), base.clone());
                        cx.new(|_| HeldCard { text, base })
                    }
                },
            )
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
                cx.stop_propagation();
                if let Some(member) = &member {
                    this.open_card(pane.as_ref(), member.clone(), opened.clone(), window, cx);
                }
            }))
            .children(reveal)
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
    /// than in the flow: a mark taking space would push every card under it
    /// down, and the aim is read off the bounds it just moved — the mark would
    /// chase the pointer it is answering.
    fn landing_mark(&self, at: Mark, cx: &Context<Self>) -> AnyElement {
        let theme = Theme::of(cx).clone();
        let mark = div().h(px(1.)).rounded_full().bg(theme.accent.opacity(0.6));
        if at == Mark::Flow {
            return mark.flex_none().into_any_element();
        }
        let mark = mark.absolute().left_0().right_0();
        match at {
            Mark::Above => mark.top(px(-4.5)),
            Mark::Top => mark.top_0(),
            Mark::Foot => mark.bottom_0(),
            _ => mark.bottom(px(-4.5)),
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

#[cfg(test)]
#[path = "../../tests/unit/board_find.rs"]
mod find_tests;

#[cfg(test)]
#[path = "../../tests/unit/board_docs.rs"]
mod doc_tests;

#[cfg(test)]
#[path = "../../tests/unit/board_preview.rs"]
mod preview_tests;

#[cfg(test)]
#[path = "../../tests/unit/board_drawer.rs"]
mod drawer_tests;
