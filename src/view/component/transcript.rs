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
    gpui::{AnyElement, Context, ScrollHandle, SharedString, Window, div, prelude::*, px},
    motion::Painter,
    theme::{TextStyle, Theme, Typeset},
    ui::{
        icons, loaders,
        scroll::{self, FollowState},
        widgets::{Layout, Status, Takeover},
    },
};
use cacp::schema::ToolKind;
use std::{
    collections::{HashMap, HashSet},
    ops::Range,
};

const CONTENT_MAX_WIDTH: f32 = 720.;

/// The transcript's breathing room at either end. The bottom carries the
/// floating composer on top of it, so the last message scrolls clear of it.
const PAD: f32 = 28.;

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
    if chat.streaming {
        zones.push(working(chat, cx));
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
        .child(scroll::rail(
            SharedString::from(format!("transcript-rail-{id}")),
            &chat.transcript.scroll,
            turns.len(),
            // The column is centred in the pane and the pane runs to the
            // window's right edge, so what is clear after the text is what is
            // clear beside it.
            window.viewport_size().width - chat.transcript.scroll.bounds().right(),
        ))
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
                .child(text.clone()),
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
            ChatItem::Agent(text) => markdown::markdown(text, window, cx),
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
    div()
        .when(!first, |el| el.border_t_1().border_color(theme.border))
        .child(
            theme
                .step_row(
                    tool_icon(*kind),
                    label.clone(),
                    None,
                    meta,
                    failed,
                    (!output.is_empty()).then_some(open),
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
        .when(open && !output.is_empty(), |el| {
            el.child(theme.step_output(("tool-output", ix), output.clone()))
        })
        .into_any_element()
}

/// The turn in flight, while it has produced nothing to show yet.
fn working(chat: &ChatSession, cx: &mut Context<Workspace>) -> AnyElement {
    let theme = Theme::of(cx).clone();
    let view = Painter::of(cx);
    div()
        .flex()
        .flex_row()
        .items_center()
        .gap(px(8.))
        .pb(px(28.))
        .child(loaders::orb(
            loaders::Orb::Cluster,
            SharedString::from(format!("working-{}", chat.id)),
            18.,
            &theme,
            view,
            cx,
        ))
        .child(
            div()
                .text_style(TextStyle::Callout)
                .text_color(theme.text_faint)
                .child("working…"),
        )
        .into_any_element()
}
