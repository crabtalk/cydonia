//! The transcript — one zone per turn: the question, the work it took, the
//! answer.
//!
//! The zone split is a `rposition`: **the answer is the prose after the last
//! tool call or thought; everything before it is interim.** That one rule is
//! what stops a model's thinking-out-loud being presented as its reply.

use crate::{
    model::{
        session::{ChatSession, nothing_said},
        workspace::Workspace,
    },
    view::root,
};
use artifact::session::chat::{ChatItem, ToolStatus};

use bezel::{
    agent::orbs::{OrbSize, OrbState, engine::Frame, orb_element},
    gpui::{
        AnyElement, Bounds, ClipboardItem, Context, Empty, Pixels, SharedString, Task, Window,
        canvas, div, prelude::*, px,
    },
    motion::Painter,
    theme::{TextStyle, Theme, Typeset, ink},
    ui::{
        icons, scroll,
        tooltip::Tooltip,
        widgets::{Icons, Layout, Status, Takeover},
    },
};
use cacp::schema::ToolKind;
use markdown::{
    BlockLayouts, Selection,
    selectable::{self, Pointer},
};
mod follow;
pub(crate) mod gallery;
pub(crate) mod links;
use std::{
    cell::{Cell, RefCell},
    collections::{HashMap, HashSet, hash_map::DefaultHasher},
    hash::{Hash, Hasher},
    ops::Range,
    rc::Rc,
    time::Duration,
};

const CONTENT_MAX_WIDTH: f32 = 720.;

/// The transcript's breathing room at either end. The bottom carries the
/// floating composer on top of it, so the last message scrolls clear of it.
const PAD: f32 = 28.;

/// The rail's marks, down the left of the pane: one dash per turn, how far it
/// stands off the edge, and the padding that carries both the gap between two
/// marks and the hitbox — a two-pixel line is not something a pointer catches.
const MARK: f32 = 16.;
const MARK_THICK: f32 = 2.;
const MARK_PAD: f32 = 5.;
const RAIL_INSET: f32 = 12.;

/// What a mark is painted at: the turn being read, one that is on screen with
/// it, and one that is neither.
///
/// Three rather than two, because the rail answers two questions at once — how
/// much of the session the pane is showing, and which turn of it you are on.
/// Off one value the second question has no answer; off two the first one is a
/// single dash whatever is on screen.
const MARK_READING: f32 = 0.6;
const MARK_VISIBLE: f32 = 0.38;
const MARK_AWAY: f32 = 0.16;

/// How much of a question its mark's tooltip carries.
const ASKED_MAX: usize = 80;

/// What the orb is drawn on: 30 frames a second while a turn is in flight,
/// claimed for a third of a second at a time and renewed by the render it
/// drives. A turn that ends stops rendering the row, the claim lapses, and the
/// app's clock parks itself.
const ORB_FPS: f32 = 30.;
const ORB_LEASE: Duration = Duration::from_millis(300);

/// The still frame an orb holds where motion is turned off — the engine's own
/// convention, and a pose rather than the empty t = 0 arrangement.
const ORB_STILL: f32 = 0.6;

/// Where a session's scrollback sits and which of its zones are open — view
/// state, per session, so switching back finds the transcript as it was left.
#[derive(Default)]
pub struct State {
    focus: RefCell<HashMap<usize, bezel::gpui::FocusHandle>>,
    galleries: RefCell<HashMap<usize, bezel::gpui::Entity<gallery::Gallery>>>,
    list: bezel::ui::list::VariableList<usize>,
    focused_turn: Cell<Option<(usize, usize)>>,
    rail_selection: Rc<Cell<Option<RailSelection>>>,
    pub(crate) footer_height: Rc<Cell<Pixels>>,
    /// Keyed by the turn's first item index.
    work: HashMap<usize, Takeover>,
    /// Tool items whose output is showing, by item index.
    output: HashSet<usize>,
    /// Pending copy feedback resets, one per message.
    copy_feedback: HashMap<usize, Task<()>>,
    /// Which item's text is selected and what of it. One at a time — a press
    /// in another item is what clears the last, the same way a page of prose
    /// has one selection however many paragraphs it holds.
    selection: Option<(usize, Selection)>,
    /// Whether the pointer is down and dragging the selection's head about.
    dragging: bool,
    /// What each item painted, so a press can be resolved against what is on
    /// screen rather than against the source.
    ///
    /// Behind a cell because the transcript is drawn from `&ChatSession`: the
    /// layouts are refilled by the renderer every frame, and an item drawn for
    /// the first time has to be able to put its own in.
    layouts: RefCell<HashMap<usize, BlockLayouts>>,
    /// The working orb's geometry, reused tick to tick. The engine overwrites
    /// it every frame, so one buffer per session is what keeps a turn in
    /// flight from growing a fresh pair of vectors thirty times a second.
    orb: Rc<RefCell<Frame>>,
    /// The same, for the sidebar's mark. Two orbs on screen are two buffers:
    /// each is filled where it is built and read back in the paint phase, so
    /// one between them would have both painting the geometry of whichever
    /// was built last.
    pub(crate) mark: Rc<RefCell<Frame>>,
}

impl State {
    /// The layout store for item `ix`, made on the first frame it is drawn.
    fn layouts(&self, ix: usize) -> BlockLayouts {
        self.layouts.borrow_mut().entry(ix).or_default().clone()
    }

    /// Put the stream back on its newest line, and keep it there as the answer
    /// arrives.
    ///
    /// Sending is the one gesture that says where you want to be looking: a
    /// wheel upward releases the tail ([`super::transcript::follow`]) and
    /// nothing puts it back, so a message sent after reading further up lands
    /// below the fold along with the reply to it.
    pub fn follow_tail(&self) {
        self.list
            .state
            .set_follow_mode(bezel::gpui::FollowMode::Tail);
    }

    /// What `ix` has selected, if it is the item holding the selection.
    fn selection(&self, ix: usize) -> Option<Selection> {
        self.selection
            .filter(|(item, _)| *item == ix)
            .map(|(_, selection)| selection)
    }

    /// Answer the pointer over item `ix`. A press starts a selection there and
    /// drops whatever another item held; a move drags its head.
    pub fn point(&mut self, ix: usize, pointer: Pointer) {
        match pointer {
            Pointer::Down(cursor) => {
                self.selection = Some((ix, Selection::at(cursor)));
                self.dragging = true;
            }
            Pointer::Move(cursor) => {
                if let Some((item, selection)) = self.selection.filter(|(item, _)| *item == ix) {
                    self.selection = Some((item, selection.extend_to(cursor)));
                }
            }
            Pointer::Up => self.dragging = false,
        }
    }

    /// Where the bar over a selection stands: the last row the run painted, in
    /// window coordinates, and the box the stream is read through.
    ///
    /// `None` while the pointer is still choosing the run, and for a press that
    /// collapsed without a drag — a caret in read-only prose is not a selection
    /// and has nothing for a bar to be about.
    pub fn selection_perch(&self) -> Option<(Bounds<Pixels>, Bounds<Pixels>)> {
        if self.dragging {
            return None;
        }
        let (ix, selection) = self.selection?;
        if selection.is_collapsed() {
            return None;
        }
        let head = *self.layouts(ix).rects(selection).last()?;
        Some((self.list.state.viewport_bounds(), head))
    }

    /// Drop the run, and the bar over it with it.
    pub fn clear_selection(&mut self) {
        self.selection = None;
        self.dragging = false;
    }

    /// What is selected, as it would be pasted, or nothing when a press
    /// collapsed without a drag behind it.
    pub fn copied(&self, chat: &ChatSession) -> Option<String> {
        let (ix, selection) = self.selection?;
        let item = chat.items.get(ix)?;
        let doc = if matches!(item, ChatItem::User(_)) {
            gallery::document(item_text(item)?).0
        } else {
            markdown::parse(item_text(item)?)
        };
        let text = selectable::copied(&doc, selection);
        (!text.is_empty()).then_some(text)
    }
}

/// Text available for selection and copying.
fn item_text(item: &ChatItem) -> Option<&str> {
    match item {
        ChatItem::User(text) | ChatItem::Agent(text) | ChatItem::Notice { text, .. } => Some(text),
        _ => None,
    }
}

/// A question and the answer it drew.
struct Turn {
    range: Range<usize>,
    /// Where the interim half ends and the reply begins.
    answer_from: usize,
}

/// Start a turn at every question. The leading chunk of a session has none —
/// a connection that failed before the first prompt is still something to show.
fn turns(items: &[ChatItem]) -> Vec<Turn> {
    let mut turns = Vec::new();
    let mut start = 0;
    for ix in 1..=items.len() {
        if ix < items.len() && !matches!(items[ix], ChatItem::User(_)) {
            continue;
        }
        // Startup stderr is not a conversation turn and has no rail mark.
        if nothing_said(&items[start..ix]) {
            start = ix;
            continue;
        }
        let interim =
            |item: &ChatItem| matches!(item, ChatItem::Tool { .. } | ChatItem::Thinking { .. });
        let answer_from = items[start..ix]
            .iter()
            .rposition(interim)
            .map_or(start, |last| start + last + 1);
        turns.push(Turn {
            range: start..ix,
            answer_from: answer_from
                .max(start + usize::from(matches!(items[start], ChatItem::User(_)))),
        });
        start = ix;
    }
    turns
}

/// User messages, agent responses, and session notices share selectable prose.
///
/// The session id rides in the closure rather than the item: a pointer event
/// arrives at the workspace, which holds every session, and the transcript on
/// screen is only one of them.
fn prose(
    chat: &ChatSession,
    ix: usize,
    text: &str,
    window: &mut Window,
    cx: &mut Context<Workspace>,
) -> AnyElement {
    let id = chat.id;
    let doc = if matches!(chat.items.get(ix), Some(ChatItem::User(_))) {
        gallery::document(text).0
    } else {
        markdown::parse(text)
    };
    let layouts = chat.transcript.layouts(ix);
    let body = selectable::render(
        ("transcript-prose", ix),
        &doc,
        &layouts,
        chat.transcript.selection(ix),
        chat.transcript.dragging,
        window,
        cx,
        move |workspace, pointer, cx| {
            workspace.with_session(id, cx, |chat| chat.transcript.point(ix, pointer));
        },
    );
    let cwd = chat.cwd.clone();
    let focus = chat
        .transcript
        .focus
        .borrow_mut()
        .entry(ix)
        .or_insert_with(|| cx.focus_handle())
        .clone();
    selectable::surface(&focus, &doc, chat.transcript.selection(ix), cx)
        .capture_any_mouse_up(cx.listener(
            move |workspace, event: &bezel::gpui::MouseUpEvent, window, cx| {
                if event.button != bezel::gpui::MouseButton::Left {
                    return;
                }
                let Some(cursor) = layouts.hit(event.position) else {
                    return;
                };
                let Some(text) = doc
                    .blocks
                    .get(cursor.block)
                    .and_then(|block| block.text_at(cursor.part))
                else {
                    return;
                };
                for span in &text.marks {
                    let markdown::Mark::Link(href) = &span.mark else {
                        continue;
                    };
                    let selection = Selection::new(
                        markdown::Cursor {
                            offset: span.range.start,
                            ..cursor
                        },
                        markdown::Cursor {
                            offset: span.range.end,
                            ..cursor
                        },
                    );
                    if !layouts
                        .rects(selection)
                        .iter()
                        .any(|bounds| bounds.contains(&event.position))
                    {
                        continue;
                    }
                    let Some((path, line)) = links::resolve(&cwd, href) else {
                        continue;
                    };
                    let clicked = workspace
                        .session(id)
                        .and_then(|chat| chat.transcript.selection(ix))
                        .is_some_and(|selection| {
                            selection.is_collapsed() && selection.head == cursor
                        });
                    workspace.with_session(id, cx, |chat| chat.transcript.point(ix, Pointer::Up));
                    cx.stop_propagation();
                    if clicked {
                        window.dispatch_action(
                            Box::new(links::OpenSessionFile {
                                session: id,
                                path,
                                line,
                            }),
                            cx,
                        );
                    }
                    return;
                }
            },
        ))
        .child(body)
        .into_any_element()
}

/// The glyph for a tool's category — what the ACP `kind` is for.
fn tool_icon(kind: ToolKind) -> &'static [u8] {
    match kind {
        ToolKind::Read => icons::text::Book,
        ToolKind::Edit => icons::text::Pen,
        ToolKind::Delete => icons::files::Trash,
        ToolKind::Move => icons::arrows::ArrowRight,
        ToolKind::Search => icons::text::Search,
        ToolKind::Execute => icons::development::Terminal,
        ToolKind::Think => icons::devices::Cpu,
        ToolKind::Fetch => icons::navigation::Globe,
        ToolKind::SwitchMode => icons::account::SlidersHorizontal,
        _ => icons::layout::LayoutGrid,
    }
}

/// The transcript of one session, rendered from the model that owns it —
/// expanding a work section or a tool's output writes back through `cx`.
pub fn render(
    chat: &ChatSession,
    pane_width: f32,
    queued: impl Fn(&mut Window, &mut bezel::gpui::App) -> Option<AnyElement> + 'static,
    _window: &mut Window,
    cx: &mut Context<Workspace>,
) -> AnyElement {
    let id = chat.id;
    let turns = turns(&chat.items);
    let list = chat.transcript.list.clone();
    let mut keys: Vec<_> = turns.iter().map(|turn| turn.range.start).collect();
    keys.push(usize::MAX);
    list.sync(keys.clone());
    for ix in list
        .visible_range()
        .chain(turns.len().saturating_sub(1)..keys.len())
    {
        if let Some(key) = keys.get(ix) {
            list.invalidate(key);
        }
    }
    let selected_turn = chat.transcript.selection.and_then(|(item, _)| {
        turns
            .iter()
            .position(|turn| turn.range.contains(&item))
            .map(|turn| (turn, item))
    });
    let previous_focus = chat.transcript.focused_turn.replace(selected_turn);
    if previous_focus != selected_turn {
        if let Some((index, _)) = previous_focus.filter(|(ix, _)| *ix < keys.len()) {
            list.focus_item(index, None);
        }
        if let Some((item, _)) = chat.transcript.selection
            && let Some((index, _)) = selected_turn
        {
            list.focus_item(index, chat.transcript.focus.borrow().get(&item).cloned());
        }
    }
    let footer_height = chat
        .transcript
        .footer_height
        .get()
        .max(px(root::composer_height()))
        + px(root::COMPOSER_BOTTOM);
    list.set_end_inset(footer_height);
    let workspace = cx.entity().downgrade();
    let visible_workspace = workspace.clone();
    let count = turns.len();
    let virtual_content = list.render(
        move |index, window, cx| {
            if index == count {
                return content_row(
                    div()
                        .children(queued(window, cx))
                        .h_auto()
                        .pb(px(PAD) + footer_height),
                )
                .into_any_element();
            }
            workspace
                .update(cx, |workspace, cx| {
                    let Some(chat) = workspace.session(id) else {
                        return Empty.into_any_element();
                    };
                    let Some(turn) = turns.get(index) else {
                        return Empty.into_any_element();
                    };
                    let running = chat.streaming && index + 1 == count;
                    content_row(
                        div()
                            .px(px(24.))
                            .when(index == 0, |row| row.pt(px(PAD)))
                            .child(zone(chat, turn, running, window, cx))
                            .when(running && turn.range.len() <= 1, |row| {
                                row.child(working(chat, turn.range.start, cx))
                            }),
                    )
                    .into_any_element()
                })
                .unwrap_or_else(|_| Empty.into_any_element())
        },
        move |range, window, cx| {
            let _ = visible_workspace.update(cx, |workspace, cx| {
                if let Some(chat) = workspace.session(id) {
                    let turns = turns_for_cache(&chat.items, range);
                    let selected = chat.transcript.selection.map(|(ix, _)| ix);
                    chat.transcript
                        .focus
                        .borrow_mut()
                        .retain(|ix, _| turns.contains(ix) || selected == Some(*ix));
                    chat.transcript
                        .layouts
                        .borrow_mut()
                        .retain(|ix, _| turns.contains(ix) || selected == Some(*ix));
                    chat.transcript
                        .galleries
                        .borrow_mut()
                        .retain(|ix, gallery| {
                            turns.contains(ix) || gallery.read(cx).is_preview_open()
                        });
                }
            });
            window.request_animation_frame();
        },
    );
    let transcript = div()
        .flex_1()
        .min_h_0()
        .relative()
        .flex()
        // The list owns the scrollbar, so only its rows constrain content width.
        .child(follow::viewport(&list.state, virtual_content))
        .child(rail(
            chat,
            &self::turns(&chat.items),
            px(rail_room(pane_width)),
        ))
        .into_any_element();
    div()
        .flex_1()
        .min_h_0()
        .flex()
        .flex_col()
        .children(chat.fork.as_ref().map(|fork| {
            let source = fork.session.clone();
            let theme = Theme::of(cx).clone();
            div()
                .id("fork-origin")
                .self_start()
                .px(px(24.))
                .pt(px(8.))
                .text_style(TextStyle::Caption)
                .text_color(theme.text_muted)
                .child(format!(
                    "Forked from {}",
                    if fork.title.is_empty() {
                        "session"
                    } else {
                        &fork.title
                    }
                ))
                .cursor_pointer()
                .hover(|link| link.text_color(theme.text))
                .on_click(cx.listener(move |workspace, _, _, cx| {
                    if let Some(id) = workspace.session_by_record(&source).map(|chat| chat.id) {
                        workspace.select_session(id, cx);
                    }
                }))
                .into_any_element()
        }))
        .child(transcript)
        .into_any_element()
}

fn content_row(content: impl IntoElement) -> bezel::gpui::Div {
    div()
        .w_full()
        .flex()
        .justify_center()
        .child(div().w_full().max_w(px(CONTENT_MAX_WIDTH)).child(content))
}

/// Retain nearby turns so small scrolls can reuse their layout and gallery state.
fn turns_for_cache(items: &[ChatItem], range: Range<usize>) -> Range<usize> {
    let turns = turns(items);
    let start = range.start.saturating_sub(2);
    let end = (range.end + 2).min(turns.len());
    turns
        .get(start)
        .map(|turn| turn.range.start)
        .unwrap_or(items.len())
        ..turns
            .get(end.saturating_sub(1))
            .map(|turn| turn.range.end)
            .unwrap_or(items.len())
}

#[derive(Clone, Copy)]
struct RailSelection {
    turn: usize,
    offset: Option<bezel::gpui::ListOffset>,
}

fn active_list_turn(
    list: &bezel::ui::list::VariableList<usize>,
    count: usize,
    inset: Pixels,
    selection: &Cell<Option<RailSelection>>,
) -> usize {
    if let Some(selected) = selection.get() {
        let current = list.state.logical_scroll_top();
        if selected.turn < count
            && selected.offset.is_none_or(|offset| {
                offset.item_ix == current.item_ix && offset.offset_in_item == current.offset_in_item
            })
        {
            return selected.turn;
        }
        selection.set(None);
    }
    let last = count.saturating_sub(1);
    if list.state.is_following_tail() {
        return last;
    }
    let top = list.state.logical_scroll_top().item_ix.min(last);
    let max = list.state.max_offset_for_scrollbar().y;
    if max <= px(0.) {
        return top;
    }
    let progress = (-list.state.scroll_px_offset_for_scrollbar().y / max).clamp(0., 1.);
    let viewport = list.state.viewport_bounds();
    // Move the reading anchor down the viewport so short trailing turns are reachable.
    let anchor = viewport.top() + (viewport.size.height - inset).max(px(0.)) * progress;
    let mut active = top;
    for ix in top..count {
        let Some(bounds) = list.state.bounds_for_item(ix) else {
            break;
        };
        if bounds.top() > anchor {
            break;
        }
        active = ix;
    }
    active
}

/// Which turns the pane is showing, as a range over the rail's marks.
///
/// `inset` is the composer band, taken off the foot: a turn behind it is
/// painted and covered, and a mark lit for it says the pane is showing
/// something it is not.
///
/// The list reports no bounds for a turn above the scroll top, which is what
/// ends the run at the near edge; the far edge is the first turn whose top is
/// past the floor.
fn visible_list_turns(
    list: &bezel::ui::list::VariableList<usize>,
    count: usize,
    inset: Pixels,
) -> Range<usize> {
    let viewport = list.state.viewport_bounds();
    let floor = viewport.bottom() - inset;
    let first = list.state.logical_scroll_top().item_ix.min(count);
    let mut last = first;
    for ix in first..count {
        let Some(bounds) = list.state.bounds_for_item(ix) else {
            break;
        };
        if bounds.top() >= floor {
            break;
        }
        last = ix + 1;
    }
    first..last.max(first)
}

fn rail_room(pane_width: f32) -> f32 {
    ((pane_width - CONTENT_MAX_WIDTH) / 2.).max(0.)
}

/// One clickable mark per turn, with its question as the tooltip.
fn rail(chat: &ChatSession, turns: &[Turn], room: Pixels) -> AnyElement {
    // The marks' padding reaches toward the text, so it comes off the room
    // before bezel is asked: a hitbox over the prose would swallow presses
    // meant for it.
    if turns.is_empty() || !scroll::rail_fits(room - px(2. * MARK_PAD)) {
        return Empty.into_any_element();
    }
    let handle = chat.transcript.list.clone();
    let count = turns.len();
    let selection = chat.transcript.rail_selection.clone();
    let inset = chat
        .transcript
        .footer_height
        .get()
        .max(px(root::composer_height()))
        + px(root::COMPOSER_BOTTOM + PAD);
    let at = active_list_turn(&handle, count, inset, &selection);
    let showing = visible_list_turns(&handle, count, inset);
    // The canvas below watches for the run moving under it, and holds its own
    // copy: the marks are built from theirs after it is mounted.
    let watched = showing.clone();
    div()
        .absolute()
        .top_0()
        .bottom_0()
        .left(px(RAIL_INSET))
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .overflow_hidden()
        .child(
            canvas(
                move |_, window, _| {
                    // Capture the actual position after a tick jump is clamped by layout.
                    if let Some(mut selected) = selection.get()
                        && selected.offset.is_none()
                    {
                        selected.offset = Some(handle.state.logical_scroll_top());
                        selection.set(Some(selected));
                    }
                    if active_list_turn(&handle, count, inset, &selection) != at
                        || visible_list_turns(&handle, count, inset) != watched
                    {
                        window.refresh();
                    }
                },
                |_, _, _, _| {},
            )
            .absolute(),
        )
        .children(turns.iter().enumerate().map(|(ix, turn)| {
            // A turn opens on a question, except the leading chunk of a
            // session — which is whatever arrived before the first one, and has
            // nothing to name itself with.
            let asked = match chat.items.get(turn.range.start) {
                Some(ChatItem::User(text)) => Some(SharedString::from(clipped(text, ASKED_MAX))),
                _ => None,
            }
            .filter(|asked| !asked.is_empty());
            let handle = chat.transcript.list.clone();
            let selection = chat.transcript.rail_selection.clone();
            div()
                .id(("rail-mark", ix))
                // Padding provides the hitbox and gap; the tone is what the
                // mark says — see [`MARK_READING`].
                .p(px(MARK_PAD))
                .cursor_pointer()
                .when_some(asked, |mark, asked| {
                    mark.tooltip(move |window, cx| Tooltip::text(asked.clone(), window, cx))
                })
                .on_click(move |_, window, _| {
                    selection.set(Some(RailSelection {
                        turn: ix,
                        offset: None,
                    }));
                    handle.scroll_to(ix);
                    if ix + 1 == count {
                        handle.state.set_follow_mode(bezel::gpui::FollowMode::Tail);
                    }
                    window.refresh();
                })
                .child(
                    div()
                        .w(px(MARK))
                        .h(px(MARK_THICK))
                        .rounded_full()
                        .bg(ink(if ix == at {
                            MARK_READING
                        } else if showing.contains(&ix) {
                            MARK_VISIBLE
                        } else {
                            MARK_AWAY
                        })),
                )
        }))
        .into_any_element()
}

fn zone(
    chat: &ChatSession,
    turn: &Turn,
    running: bool,
    window: &mut Window,
    cx: &mut Context<Workspace>,
) -> AnyElement {
    let theme = Theme::of(cx).clone();
    let first = turn.range.start;
    let body =
        (first + usize::from(matches!(chat.items[first], ChatItem::User(_))))..turn.answer_from;
    let steps = chat.items[body.clone()]
        .iter()
        .filter(|item| matches!(item, ChatItem::Tool { .. }))
        .count();
    // Auto-follow while the turn runs, and the person who presses the
    // header wins from then on.
    let open = chat
        .transcript
        .work
        .get(&first)
        .copied()
        .unwrap_or_default()
        .get(running);

    let mut zone = div().flex().flex_col().gap(px(10.)).pb(px(28.));
    if let Some(ChatItem::User(text)) = chat.items.get(first) {
        let (doc, images) = gallery::document(text);
        let gallery = (!images.is_empty()).then(|| {
            chat.transcript
                .galleries
                .borrow_mut()
                .entry(first)
                .or_insert_with(|| cx.new(|cx| gallery::Gallery::new(images, &chat.cwd, cx)))
                .clone()
        });
        let group = SharedString::from(format!("user-message-{}-{first}", chat.id));
        let fork_group = SharedString::from(format!("fork-message-{}-{first}", chat.id));
        let copy_group = SharedString::from(format!("copy-message-{}-{first}", chat.id));
        let caption_size = TextStyle::Caption.painted();
        let button_size = px(caption_size * 2.);
        let copied = text.clone();
        let id = chat.id;
        let copy_done = chat.transcript.copy_feedback.contains_key(&first);
        let timestamp = chat.sent_at.get(&first).and_then(|seconds| {
            chrono::DateTime::from_timestamp(i64::try_from(*seconds).ok()?, 0).map(|time| {
                time.with_timezone(&chrono::Local)
                    .format("%-I:%M %p")
                    .to_string()
            })
        });
        zone = zone.child(
            div()
                .group(group.clone())
                .self_end()
                .max_w(px(440.))
                .when(gallery.is_some(), |row| row.w(px(440.)).max_w_full())
                .flex()
                .flex_col()
                .gap(px(4.))
                .child(
                    div()
                        .px(px(14.))
                        .py(px(9.))
                        .rounded(px(Theme::surface_radius()))
                        .bg(theme.surface_raised)
                        .text_style(TextStyle::Body)
                        .text_color(theme.text)
                        .flex()
                        .flex_col()
                        .gap(px(8.))
                        .when(!doc.blocks.is_empty(), |bubble| {
                            bubble.child(prose(chat, first, text, window, cx))
                        })
                        .children(gallery),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_end()
                        .gap(px(caption_size))
                        .pr(px(6.))
                        .text_style(TextStyle::Caption)
                        .text_color(theme.text_faint)
                        // Reserve the row so hovering does not move the conversation.
                        .invisible()
                        .group_hover(group, |row| row.visible())
                        .children(timestamp.map(|text| {
                            div()
                                .h(button_size)
                                .flex_none()
                                .flex()
                                .items_center()
                                .child(text)
                        }))
                        .child(
                            div()
                                .flex()
                                .flex_none()
                                .items_center()
                                .gap(px(caption_size * 0.2))
                                .child(
                                    div()
                                        .id(("fork-user-message", first))
                                        .group(fork_group.clone())
                                        .size(button_size)
                                        .flex_none()
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .cursor_pointer()
                                        .tooltip(|window, cx| {
                                            Tooltip::text("Fork session from here", window, cx)
                                        })
                                        .on_click(cx.listener(move |workspace, _, _, cx| {
                                            workspace.fork_session(id, first, cx);
                                        }))
                                        .child(
                                            theme
                                                .icon_at(
                                                    TextStyle::Caption,
                                                    icons::development::GitFork,
                                                )
                                                .text_color(theme.text_muted)
                                                .group_hover(fork_group, |icon| {
                                                    icon.text_color(theme.text)
                                                }),
                                        ),
                                )
                                .child(
                                    div()
                                        .id(("copy-user-message", first))
                                        .group(copy_group.clone())
                                        .size(button_size)
                                        .flex_none()
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .cursor_pointer()
                                        .tooltip(move |window, cx| {
                                            Tooltip::text(
                                                if copy_done { "Copied" } else { "Copy message" },
                                                window,
                                                cx,
                                            )
                                        })
                                        .on_click(cx.listener(move |workspace, _, _, cx| {
                                            cx.write_to_clipboard(ClipboardItem::new_string(
                                                copied.clone(),
                                            ));
                                            let reset = cx.spawn(async move |workspace, cx| {
                                                cx.background_executor()
                                                    .timer(Duration::from_secs(2))
                                                    .await;
                                                let _ = workspace.update(cx, |workspace, cx| {
                                                    workspace.with_session(id, cx, |chat| {
                                                        chat.transcript
                                                            .copy_feedback
                                                            .remove(&first);
                                                    });
                                                });
                                            });
                                            workspace.with_session(id, cx, |chat| {
                                                chat.transcript.copy_feedback.insert(first, reset);
                                            });
                                        }))
                                        .child(
                                            theme
                                                .icon_at(
                                                    TextStyle::Caption,
                                                    if copy_done {
                                                        icons::notifications::Check
                                                    } else {
                                                        icons::text::Copy
                                                    },
                                                )
                                                .text_color(theme.text_muted)
                                                .group_hover(copy_group, |icon| {
                                                    icon.text_color(theme.text)
                                                }),
                                        ),
                                ),
                        ),
                ),
        );
    }
    if !body.is_empty() {
        zone = zone.child(work_header(chat.id, first, steps, open, cx));
        if open {
            zone = zone.child(
                div()
                    .ml(px(10.))
                    .pl(px(12.))
                    .border_l_1()
                    .border_color(theme.border)
                    .flex()
                    .flex_col()
                    .gap(px(8.))
                    .children(work(chat, body, cx)),
            );
        }
    }
    for ix in turn.answer_from..turn.range.end {
        zone = zone.child(match &chat.items[ix] {
            ChatItem::Agent(text) => prose(chat, ix, text, window, cx),
            ChatItem::Notice { text, .. } => div()
                .opacity(0.65)
                .child(prose(chat, ix, text, window, cx))
                .into_any_element(),
            ChatItem::Process { command, output } => {
                let (command, output) = (command.clone(), output.clone());
                process(chat, ix, &command, &output, cx)
            }
            _ => div().into_any_element(),
        });
    }
    zone.into_any_element()
}

/// How much happened, and a chevron to see it.
fn work_header(
    id: u64,
    turn: usize,
    steps: usize,
    open: bool,
    cx: &mut Context<Workspace>,
) -> AnyElement {
    let theme = Theme::of(cx).clone();
    let label = match steps {
        0 => "Thought".to_owned(),
        1 => "Worked · 1 step".to_owned(),
        n => format!("Worked · {n} steps"),
    };
    div()
        .id(("work", turn))
        .self_start()
        .flex()
        .flex_row()
        .items_center()
        .gap(px(6.))
        .px(px(4.))
        .py(px(5.))
        .rounded(px(Theme::control_radius()))
        .cursor_pointer()
        .hover(|el| el.bg(theme.element_hover))
        .on_click(cx.listener(move |this, _, _, cx| {
            this.with_session(id, cx, |chat| {
                let running = chat.streaming;
                chat.transcript
                    .work
                    .entry(turn)
                    .or_default()
                    .toggle(running);
            });
        }))
        .child(theme.disclosure(open))
        .child(
            div()
                .text_style(TextStyle::Callout)
                .text_color(theme.text_muted)
                .child(label),
        )
        .into_any_element()
}

/// The interim half: thoughts, prose, and runs of adjacent tool calls boxed
/// together — the run boundary is "is this a tool", so a sentence between
/// two calls breaks the box exactly where it should.
fn work(chat: &ChatSession, body: Range<usize>, cx: &mut Context<Workspace>) -> Vec<AnyElement> {
    let theme = Theme::of(cx).clone();
    let is_tool = |item: &ChatItem| matches!(item, ChatItem::Tool { .. });
    let mut out = Vec::new();
    let mut ix = body.start;
    for run in chat.items[body].chunk_by(|a, b| is_tool(a) == is_tool(b)) {
        if is_tool(&run[0]) {
            let start = ix;
            out.push(
                div()
                    .rounded(px(Theme::panel_radius()))
                    .border_1()
                    .border_color(theme.border)
                    .overflow_hidden()
                    .children((start..start + run.len()).map(|i| tool(chat, i, i == start, cx)))
                    .into_any_element(),
            );
        } else {
            out.extend(run.iter().map(|item| {
                match item {
                    ChatItem::Thinking { text, .. } => div()
                        .flex()
                        .flex_row()
                        .items_start()
                        .gap(px(6.))
                        .text_style(TextStyle::Callout)
                        .text_color(theme.text_muted.opacity(0.7))
                        .child(
                            icons::icon(icons::devices::Cpu)
                                .size(px(12.))
                                .text_color(theme.text_faint),
                        )
                        .child(text.clone())
                        .into_any_element(),
                    ChatItem::Agent(text) | ChatItem::Notice { text, .. } => div()
                        .text_style(TextStyle::Callout)
                        .text_color(theme.text_muted)
                        .when(matches!(item, ChatItem::Notice { .. }), |el| {
                            el.opacity(0.65)
                        })
                        .child(text.clone())
                        .into_any_element(),
                    _ => div().into_any_element(),
                }
            }));
        }
        ix += run.len();
    }
    out
}

/// How far a title's head — the word that names the call — may run before the
/// rest of the line has to start truncating.
pub const HEAD_MAX: usize = 24;

/// How long a one-lined title may run before the row's ellipsis is the only
/// way to read it, and the full text is worth opening for.
const TITLE_MAX: usize = 72;

/// A tool's title, as a row one line tall can take it: the head that names the
/// call, the rest that truncates beside it, and the whole text when the one
/// line lost something — a shell script arrives as its own title, newlines and
/// all.
pub fn title(label: &str) -> (SharedString, Option<SharedString>, Option<SharedString>) {
    let label = label.trim();
    let line = one_line(label);
    let cut = line
        .char_indices()
        .nth(HEAD_MAX)
        .map_or(line.len(), |(at, _)| at);
    let head = line[..cut].find(char::is_whitespace).unwrap_or(cut);
    let (name, rest) = line.split_at(head);
    let rest = rest.trim();
    (
        SharedString::from(name.to_owned()),
        (!rest.is_empty()).then(|| SharedString::from(rest.to_owned())),
        (line != label || line.chars().count() > TITLE_MAX)
            .then(|| SharedString::from(label.to_owned())),
    )
}

/// `text` with every run of whitespace — newlines included — as one space.
fn one_line(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The same, cut to `max` characters and ellipsized.
fn clipped(text: &str, max: usize) -> String {
    let line = one_line(text);
    match line.char_indices().nth(max) {
        Some((at, _)) => format!("{}…", line[..at].trim_end()),
        None => line,
    }
}

/// One tool call, and what it printed.
fn tool(chat: &ChatSession, ix: usize, first: bool, cx: &mut Context<Workspace>) -> AnyElement {
    let theme = Theme::of(cx).clone();
    let ChatItem::Tool {
        kind,
        label,
        status,
        output,
        ..
    } = &chat.items[ix]
    else {
        return div().into_any_element();
    };
    let id = chat.id;
    let open = chat.transcript.output.contains(&ix);
    let failed = *status == ToolStatus::Failure;
    let meta = (*status == ToolStatus::Running).then(|| SharedString::from("running"));
    let (name, rest, full) = title(label);
    div()
        .when(!first, |el| el.border_t_1().border_color(theme.border))
        .child(
            theme
                .step_row(
                    tool_icon(*kind),
                    name,
                    rest,
                    meta,
                    failed,
                    (!output.is_empty() || full.is_some()).then_some(open),
                )
                .hover(|el| el.bg(theme.element_hover))
                .id(("tool", ix))
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.with_session(id, cx, |chat| {
                        if !chat.transcript.output.insert(ix) {
                            chat.transcript.output.remove(&ix);
                        }
                    });
                })),
        )
        // What the row could not hold, in the order it was read: the call
        // itself, then what it printed.
        .when_some(full.filter(|_| open), |el, full| {
            el.child(theme.step_output(("tool-title", ix), full))
        })
        .when(open && !output.is_empty(), |el| {
            el.child(theme.step_output(("tool-output", ix), output.clone()))
        })
        .into_any_element()
}

/// The agent process itself, and what it printed.
///
/// The same two elements a tool call gets, because it is the same thing to
/// read: something was executed, and this is what came back. Not folded in
/// with the tool calls, though — those belong to a turn and collapse with it,
/// and this belongs to the process the whole session is running on.
fn process(
    chat: &ChatSession,
    ix: usize,
    command: &str,
    output: &str,
    cx: &mut Context<Workspace>,
) -> AnyElement {
    let theme = Theme::of(cx).clone();
    let id = chat.id;
    let open = chat.transcript.output.contains(&ix);
    let (name, rest, full) = title(command);
    div()
        .child(
            theme
                .step_row(
                    tool_icon(ToolKind::Execute),
                    name,
                    rest,
                    None,
                    false,
                    (!output.is_empty() || full.is_some()).then_some(open),
                )
                .hover(|el| el.bg(theme.element_hover))
                .id(("process", ix))
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.with_session(id, cx, |chat| {
                        if !chat.transcript.output.insert(ix) {
                            chat.transcript.output.remove(&ix);
                        }
                    });
                })),
        )
        .when_some(full.filter(|_| open), |el, full| {
            el.child(theme.step_output(("process-title", ix), full))
        })
        .when(open && !output.is_empty(), |el| {
            el.child(theme.step_output(
                ("process-output", ix),
                SharedString::from(output.to_owned()),
            ))
        })
        .into_any_element()
}

/// What a turn in flight is called while it has nothing to show yet.
///
/// Taken from `../desktop`, which settled this first.
///
/// A list rather than one word, and long enough that the same one twice reads
/// as chance. Which one a turn gets is [`verb`]'s business.
const WORKING: [&str; 40] = [
    "Thinking",
    "Brewing",
    "Cultivating",
    "Simmering",
    "Percolating",
    "Distilling",
    "Weaving",
    "Conjuring",
    "Steeping",
    "Fermenting",
    "Crystallizing",
    "Synthesizing",
    "Composing",
    "Pondering",
    "Unraveling",
    "Forging",
    "Kindling",
    "Gathering",
    "Polishing",
    "Assembling",
    "Decoding",
    "Untangling",
    "Refining",
    "Shaping",
    "Hatching",
    "Coalescing",
    "Contemplating",
    "Illuminating",
    "Molding",
    "Calibrating",
    "Churning",
    "Marinating",
    "Incubating",
    "Digesting",
    "Sprouting",
    "Condensing",
    "Mulling",
    "Concocting",
    "Ruminating",
    "Orchestrating",
];

/// Which word a turn gets: the question's own hash.
///
/// Stable, because it has to be — a word rerolled per frame would be a spinner
/// made of text, and the transcript repaints every 120ms while a turn streams.
/// Not the turn's position, which was the first thing tried and which makes the
/// first turn of every session the first word in the list.
///
/// Hashing what was *asked* gets the variety a random pick would, and keeps it
/// for as long as the question is on screen.
fn verb(question: &str) -> &'static str {
    WORKING[asked_hash(question) % WORKING.len()]
}

pub fn working_word(chat: &ChatSession) -> &'static str {
    let question = chat
        .items
        .iter()
        .rev()
        .find(|item| matches!(item, ChatItem::User(_)))
        .and_then(item_text)
        .unwrap_or_default();
    verb(question)
}

/// Which orb a session's turn in flight wears, off the same hash for the same
/// reason: a shape rerolled per frame would be twelve loaders playing at once,
/// and one picked per place would have the sidebar and the transcript showing
/// the same turn as two different things.
///
/// The word and the shape are picked apart rather than paired — there are
/// forty of one and twelve of the other, and pairing them would cost the words
/// their variety.
pub fn orb_of(chat: &ChatSession) -> OrbState {
    // The turn in flight opened at the last thing asked — which is where
    // `turns` starts one too, and 0 for a session that was never asked
    // anything.
    let at = chat
        .items
        .iter()
        .rposition(|item| matches!(item, ChatItem::User(_)))
        .unwrap_or(0);
    let question = chat.items.get(at).and_then(item_text).unwrap_or_default();
    OrbState::ALL_STATES[asked_hash(question) % OrbState::ALL_STATES.len()]
}

/// That orb, drawn, at the time the turn has been running — unbounded, which
/// is what the engine wants, and starting from nothing every turn.
///
/// The orb paints one frame of a clock it does not keep, so this asks for the
/// frames itself. The claim is renewed by the render it drives rather than
/// held anywhere, so it lapses on its own the moment the orb stops being
/// drawn.
pub fn orb<V: 'static>(
    state: OrbState,
    since: Duration,
    frame: &Rc<RefCell<Frame>>,
    cx: &mut Context<V>,
) -> AnyElement {
    Painter::of(cx).lease(ORB_FPS, ORB_LEASE, cx);
    let t = match cx.reduce_motion() {
        true => ORB_STILL,
        false => since.as_secs_f32(),
    };
    orb_element(state, OrbSize::Inline, t, frame).into_any_element()
}

/// The question, as a number to pick with.
fn asked_hash(question: &str) -> usize {
    let mut hash = DefaultHasher::new();
    question.hash(&mut hash);
    hash.finish() as usize
}

/// A turn's age, in the coarsest unit that still says something.
fn since(elapsed: Duration) -> String {
    let secs = elapsed.as_secs();
    match secs < 60 {
        true => format!("{secs}s"),
        false => format!("{}m {}s", secs / 60, secs % 60),
    }
}

/// Tokens, thinned to the digits that carry: `840`, `4.2k`, `128k`.
fn tokens(spent: u64) -> String {
    match spent {
        n if n < 1_000 => n.to_string(),
        n if n < 100_000 => format!("{:.1}k", n as f64 / 1_000.),
        n => format!("{}k", n / 1_000),
    }
}

/// What the turn has cost so far, quieter than the word it follows. Absent
/// until there is something to say — a turn that has not been running a whole
/// second yet is not news.
fn spend(chat: &ChatSession, theme: &Theme) -> Option<AnyElement> {
    let mut parts = Vec::new();
    if let Some(elapsed) = chat.elapsed().filter(|elapsed| elapsed.as_secs() > 0) {
        parts.push(since(elapsed));
    }
    // Only once it is worth a number. A turn opens on nothing spent, and
    // `0 tokens` is a fact about the clock rather than about the turn.
    if let Some(spent) = chat.spent().filter(|spent| *spent > 0) {
        parts.push(format!("{} tokens", tokens(spent)));
    }
    (!parts.is_empty()).then(|| {
        div()
            .text_style(TextStyle::Callout)
            .text_color(theme.text_faint.opacity(0.6))
            .child(format!("({})", parts.join(" · ")))
            .into_any_element()
    })
}

/// The turn in flight, while it has produced nothing to show yet. `at` is the
/// turn's first item — the question, which is what its word and its orb come
/// from.
fn working(chat: &ChatSession, at: usize, cx: &mut Context<Workspace>) -> AnyElement {
    let theme = Theme::of(cx).clone();
    let asked = chat.items.get(at).and_then(item_text).unwrap_or_default();
    let state = orb_of(chat);
    let since = chat.elapsed().unwrap_or_default();
    div()
        .flex()
        .flex_row()
        .items_center()
        .gap(px(8.))
        .pb(px(28.))
        .child(orb(state, since, &chat.transcript.orb, cx))
        .child(
            div()
                .text_style(TextStyle::Callout)
                .text_color(theme.text_faint)
                .child(format!("{}…", verb(asked))),
        )
        // The clock keeps itself: the orb turns, so this row is repainted
        // every frame whether or not the agent has said anything.
        .children(spend(chat, &theme))
        .into_any_element()
}

#[cfg(test)]
#[path = "../../../tests/unit/transcript_turns.rs"]
mod tests;

#[cfg(test)]
#[path = "../../../tests/unit/transcript_rail.rs"]
mod rail_tests;

#[cfg(test)]
#[path = "../../../tests/unit/transcript_virtual.rs"]
mod virtual_tests;

#[cfg(test)]
#[path = "../../../tests/unit/transcript_scrollbar.rs"]
mod scrollbar_tests;
