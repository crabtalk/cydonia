//! The transcript — one zone per turn: the question, the work it took, the
//! answer.
//!
//! The zone split is a `rposition`: **the answer is the prose after the last
//! tool call or thought; everything before it is interim.** That one rule is
//! what stops a model's thinking-out-loud being presented as its reply.

use crate::{
    model::{
        session::{ChatItem, ChatSession, ToolStatus},
        workspace::Workspace,
    },
    view::root,
};
use bezel::{
    agent::orbs::{OrbSize, OrbState, engine::Frame, orb_element},
    gpui::{
        AnyElement, Context, Empty, Pixels, ScrollHandle, SharedString, Window, div, prelude::*, px,
    },
    motion::Painter,
    theme::{TextStyle, Theme, Typeset, ink},
    ui::{
        icons,
        scroll::{self, FollowState},
        tooltip::Tooltip,
        widgets::{Layout, Status, Takeover},
    },
};
use cacp::schema::ToolKind;
use markdown::{
    BlockLayouts, Selection,
    selectable::{self, Pointer},
};
use std::{
    cell::RefCell,
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
    scroll: ScrollHandle,
    follow: FollowState,
    /// Keyed by the turn's first item index.
    work: HashMap<usize, Takeover>,
    /// Tool items whose output is showing, by item index.
    output: HashSet<usize>,
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

    /// What is selected, as it would be pasted, or nothing when a press
    /// collapsed without a drag behind it.
    pub fn copied(&self, chat: &ChatSession) -> Option<String> {
        let (ix, selection) = self.selection?;
        let doc = markdown::parse(item_text(chat.items.get(ix)?)?);
        let text = selectable::copied(&doc, selection);
        (!text.is_empty()).then_some(text)
    }
}

/// The prose of an item, for the two kinds that carry any.
fn item_text(item: &ChatItem) -> Option<&str> {
    match item {
        ChatItem::User(text) | ChatItem::Agent(text) => Some(text),
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
        let interim =
            |item: &ChatItem| matches!(item, ChatItem::Tool { .. } | ChatItem::Thinking { .. });
        let answer_from = items[start..ix]
            .iter()
            .rposition(interim)
            .map_or(start, |last| start + last + 1);
        turns.push(Turn {
            range: start..ix,
            answer_from: answer_from.max(start + 1),
        });
        start = ix;
    }
    turns
}

/// What the session has to say for itself, in the strip its severity earns.
fn notice(theme: &Theme, text: &str, failed: bool) -> AnyElement {
    let strip = if failed {
        theme.error_strip(SharedString::from(text.to_owned()))
    } else {
        theme.warning_strip(SharedString::from(text.to_owned()))
    };
    strip.mt(px(0.)).into_any_element()
}

/// One message, selectable. The transcript's two prose items — what you asked
/// and what came back — are the same element, because copying the one is the
/// same act as copying the other.
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
    selectable::render(
        ("transcript-prose", ix),
        &markdown::parse(text),
        &chat.transcript.layouts(ix),
        chat.transcript.selection(ix),
        chat.transcript.dragging,
        window,
        cx,
        move |workspace, pointer, cx| {
            workspace.with_session(id, cx, |chat| chat.transcript.point(ix, pointer));
        },
    )
}

/// The glyph for a tool's category — what the ACP `kind` is for.
fn tool_icon(kind: ToolKind) -> &'static str {
    match kind {
        ToolKind::Read => icons::files::BOOK,
        ToolKind::Edit => icons::editing::PEN,
        ToolKind::Delete => icons::files::TRASH_BIN_MINIMALISTIC,
        ToolKind::Move => icons::arrows::ARROW_RIGHT,
        ToolKind::Search => icons::system::MAGNIFER,
        ToolKind::Execute => icons::devices::TERMINAL,
        ToolKind::Think => icons::devices::CPU,
        ToolKind::Fetch => icons::devices::GLOBAL,
        ToolKind::SwitchMode => icons::system::TUNING,
        _ => icons::system::WIDGET,
    }
}

/// The transcript of one session, rendered from the model that owns it —
/// expanding a work section or a tool's output writes back through `cx`.
pub fn render(chat: &ChatSession, window: &mut Window, cx: &mut Context<Workspace>) -> AnyElement {
    let id = chat.id;
    let turns = turns(&chat.items);
    let last = turns.len().saturating_sub(1);
    let mut zones: Vec<AnyElement> = Vec::new();
    for (position, turn) in turns.iter().enumerate() {
        let running = chat.streaming && position == last;
        zones.push(zone(chat, turn, running, window, cx));
    }
    // Only while the turn has nothing to show. Once it has, the text arriving
    // under it *is* the sign that it is running — and a row pinned below prose
    // that reflows on every streamed frame is a row that jumps, taking the eye
    // with it. See [`working`].
    if let Some(turn) = turns
        .last()
        .filter(|turn| chat.streaming && turn.range.len() <= 1)
    {
        zones.push(working(chat, turn.range.start, cx));
    }

    div()
        .flex_1()
        .min_h_0()
        .relative()
        .flex()
        .justify_center()
        .child(
            div()
                .relative()
                .w_full()
                .max_w(px(CONTENT_MAX_WIDTH))
                .child(
                    // The turns are the scroll container's own children, not a
                    // column inside it: `scroll::rail` addresses what gpui
                    // indexes, and a wrapper would leave it one item to point at.
                    div()
                        .id(("transcript", id))
                        .size_full()
                        .overflow_y_scroll()
                        .track_scroll(&chat.transcript.scroll)
                        .px(px(24.))
                        .pt(px(PAD))
                        .pb(px(PAD + root::composer_height() + root::COMPOSER_BOTTOM))
                        .flex()
                        .flex_col()
                        .children(zones),
                )
                .child(scroll::follow(
                    &chat.transcript.scroll,
                    &chat.transcript.follow,
                )),
        )
        .child(rail(
            chat,
            &turns,
            // The column is centred in the pane and the pane runs to the
            // window's right edge, so what is clear after the text is what is
            // clear beside it.
            window.viewport_size().width - chat.transcript.scroll.bounds().right(),
        ))
        .into_any_element()
}

/// One mark per turn down the left of the pane, the turn at the top of the
/// viewport lit, and the question it opened on its tooltip. A press jumps
/// there.
///
/// `bezel::ui::scroll::rail` in every respect but the tooltip, which is the
/// whole point here: a column of identical dashes says how many turns there
/// are and nothing about which is which, and the thing a person is looking for
/// is what they asked.
fn rail(chat: &ChatSession, turns: &[Turn], room: Pixels) -> AnyElement {
    // bezel's own floor plus the marks' padding, which reaches toward the
    // text: a hitbox over the prose would swallow presses meant for it.
    if turns.is_empty() || room < px(scroll::RAIL_ROOM + 2. * MARK_PAD) {
        return Empty.into_any_element();
    }
    let at = chat.transcript.scroll.top_item();
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
        .children(turns.iter().enumerate().map(|(ix, turn)| {
            // A turn opens on a question, except the leading chunk of a
            // session — which is whatever arrived before the first one, and has
            // nothing to name itself with.
            let asked = match chat.items.get(turn.range.start) {
                Some(ChatItem::User(text)) => Some(SharedString::from(clipped(text, ASKED_MAX))),
                _ => None,
            }
            .filter(|asked| !asked.is_empty());
            let handle = chat.transcript.scroll.clone();
            div()
                .id(("rail-mark", ix))
                // The padding is the hitbox and the gap between two marks at
                // once; the dash inside it brightens for the whole of it.
                .p(px(MARK_PAD))
                .group("rail-mark")
                .cursor_pointer()
                .when_some(asked, |mark, asked| {
                    mark.tooltip(move |window, cx| Tooltip::text(asked.clone(), window, cx))
                })
                .on_click(move |_, window, _| {
                    handle.scroll_to_item(ix);
                    window.refresh();
                })
                .child(
                    div()
                        .w(px(MARK))
                        .h(px(MARK_THICK))
                        .rounded_full()
                        .bg(if ix == at { ink(0.6) } else { ink(0.2) })
                        .group_hover("rail-mark", |mark| mark.bg(ink(0.32))),
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
    let body = (first + 1).min(turn.range.end)..turn.answer_from;
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
        zone = zone.child(
            div()
                .self_end()
                .max_w(px(440.))
                .px(px(14.))
                .py(px(9.))
                .rounded(px(Theme::surface_radius()))
                .bg(theme.surface_raised)
                .text_style(TextStyle::Body)
                .text_color(theme.text)
                .child(prose(chat, first, text, window, cx)),
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
            ChatItem::Notice { text, failed } => notice(&theme, text, *failed),
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
                            icons::icon(icons::devices::CPU)
                                .size(px(12.))
                                .text_color(theme.text_faint),
                        )
                        .child(text.clone())
                        .into_any_element(),
                    ChatItem::Agent(text) => div()
                        .text_style(TextStyle::Callout)
                        .text_color(theme.text_muted)
                        .child(text.clone())
                        .into_any_element(),
                    ChatItem::Notice { text, failed } => notice(&theme, text, *failed),
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
const HEAD_MAX: usize = 24;

/// How long a one-lined title may run before the row's ellipsis is the only
/// way to read it, and the full text is worth opening for.
const TITLE_MAX: usize = 72;

/// A tool's title, as a row one line tall can take it: the head that names the
/// call, the rest that truncates beside it, and the whole text when the one
/// line lost something — a shell script arrives as its own title, newlines and
/// all.
fn title(label: &str) -> (SharedString, Option<SharedString>, Option<SharedString>) {
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
mod tests {
    use super::title;

    /// A shell script arrives as its own title. The row gets one line of it;
    /// the whole thing is what opening the row is for.
    #[test]
    fn a_script_is_one_line_and_a_way_back_to_the_rest() {
        let script = "python3 -c \"\nimport json\nprint(json.dumps({}))\n\"";
        let (name, rest, full) = title(script);
        assert_eq!(name, "python3");
        assert_eq!(
            rest.expect("the command after the head"),
            "-c \" import json print(json.dumps({})) \""
        );
        assert_eq!(full.expect("the script, as written"), script);
    }

    /// A title that already fits keeps its head and its rest, and has nothing
    /// left over to open onto.
    #[test]
    fn a_short_title_opens_onto_nothing() {
        let (name, rest, full) = title("Read src/view/root.rs");
        assert_eq!(name, "Read");
        assert_eq!(rest.expect("the path"), "src/view/root.rs");
        assert!(full.is_none());
    }

    /// One long word is still one line: the head is capped so the rest has
    /// somewhere to truncate.
    #[test]
    fn a_head_longer_than_the_row_is_cut() {
        let (name, rest, _) = title(&"x".repeat(40));
        assert_eq!(name.len(), super::HEAD_MAX);
        assert_eq!(rest.expect("what the head could not take").len(), 16);
    }
}
