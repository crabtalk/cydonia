//! The search palette: every open project's articles, boards and sessions,
//! raised from the sidebar's Search row.
//!
//! An empty query lists what was touched last. A query is searched off disk on
//! a background thread — see [`artifact::search::disk`] — so an entry's hits
//! are as of its last write. Opening a hit in an entry's body puts the query in
//! its pane's find bar — see [`crate::view::find`].

use crate::view::{
    keymap::{self, Command},
    root::Cydonia,
    sidebar::{self, Row},
};
use artifact::{
    project::fs,
    search::{self, Block, Kind, Query},
};
use bezel::{
    gpui::{
        self, AnyElement, App, Context, Entity, Focusable as _, FontWeight, HighlightStyle,
        KeyBinding, ScrollHandle, SharedString, StyledText, Task, Window, actions, div, prelude::*,
        px,
    },
    theme::{TextStyle, Theme, Typeset as _},
    ui::{
        icons::{self, Icon},
        input::{FieldEvent, TextField},
        popover,
    },
};
use std::{collections::HashMap, ops::Range, path::PathBuf, sync::mpsc, time::Duration};

actions!(
    cydonia_search,
    [
        ToggleSearch,
        DismissSearch,
        SelectNext,
        SelectPrev,
        OpenHit,
        NextFilter,
        PrevFilter
    ]
);

const CONTEXT: &str = "CydoniaSearch";

/// Rows an empty query lists.
const RECENT: usize = 12;

/// Rows a query lists, at most.
const SHOWN: usize = 50;

/// How long typing has to pause before the query is searched.
const SETTLE: Duration = Duration::from_millis(80);

/// Characters of a snippet kept ahead of the match.
const LEAD: usize = 32;

pub fn bindings() -> Vec<KeyBinding> {
    let ctx = Some(CONTEXT);
    vec![
        KeyBinding::new("escape", DismissSearch, ctx),
        KeyBinding::new("down", SelectNext, ctx),
        KeyBinding::new("ctrl-n", SelectNext, ctx),
        KeyBinding::new("up", SelectPrev, ctx),
        KeyBinding::new("ctrl-p", SelectPrev, ctx),
        KeyBinding::new("enter", OpenHit, ctx),
        KeyBinding::new("tab", NextFilter, ctx),
        KeyBinding::new("shift-tab", PrevFilter, ctx),
    ]
}

/// One row of the palette.
#[derive(Clone)]
pub(crate) struct Hit {
    row: Row,
    /// Where the query was found, when it was found in the entry's body: the
    /// line holding it, and where in the line.
    snippet: Option<(String, Range<usize>)>,
}

/// An entry's best match as the search found it.
struct Found {
    root: PathBuf,
    kind: Kind,
    id: String,
    in_title: bool,
    snippet: Option<(String, Range<usize>)>,
}

pub(crate) struct Search {
    open: bool,
    field: Entity<TextField>,
    /// What the last finished listing found. Kept on screen while the next
    /// one runs, so typing does not empty the palette between keystrokes.
    hits: Vec<Hit>,
    /// Whether a search for the field's query is still running.
    searching: bool,
    /// The one kind listed, or every kind.
    filter: Option<Kind>,
    /// Rows listed at most, once filtered.
    limit: usize,
    selected: usize,
    scroll: ScrollHandle,
    task: Option<Task<()>>,
}

impl Search {
    pub(crate) fn new(cx: &mut Context<Cydonia>) -> Self {
        let field = cx.new(|cx| {
            TextField::new(cx)
                .with_frame(false)
                .with_key_context(CONTEXT)
                .with_placeholder("Search articles, boards and sessions…")
        });
        cx.subscribe(&field, |this, _, event: &FieldEvent, cx| {
            if matches!(event, FieldEvent::Changed(_)) {
                this.refresh_search(cx);
            }
        })
        .detach();
        Self {
            open: false,
            field,
            hits: Vec::new(),
            searching: false,
            filter: None,
            limit: RECENT,
            selected: 0,
            scroll: ScrollHandle::new(),
            task: None,
        }
    }

    /// The hits the filter leaves, in order.
    fn shown(&self) -> Vec<&Hit> {
        self.hits
            .iter()
            .filter(|hit| {
                self.filter
                    .is_none_or(|kind| kind_of(hit.row) == Some(kind))
            })
            .take(self.limit)
            .collect()
    }

    fn query(&self, cx: &App) -> Option<Query> {
        Query::literal(self.field.read(cx).content())
    }
}

impl Cydonia {
    pub(crate) fn toggle_search(
        &mut self,
        _: &ToggleSearch,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.search.open {
            self.dismiss_search(&DismissSearch, window, cx);
            return;
        }
        self.search.open = true;
        self.search.field.update(cx, |field, cx| field.clear(cx));
        self.refresh_search(cx);
        window.focus(&self.search.field.read(cx).focus_handle(cx), cx);
        cx.notify();
    }

    fn dismiss_search(&mut self, _: &DismissSearch, window: &mut Window, cx: &mut Context<Self>) {
        self.search.open = false;
        self.search.task = None;
        window.focus(&self.leaf().focus, cx);
        cx.notify();
    }

    /// List again for what the field holds. A search still running is
    /// dropped with its task.
    fn refresh_search(&mut self, cx: &mut Context<Self>) {
        let Some(query) = self.search.query(cx) else {
            self.search.task = None;
            self.search.searching = false;
            self.search.hits = self.recent_hits(cx);
            self.search.limit = RECENT;
            self.search.selected = 0;
            cx.notify();
            return;
        };
        self.search.searching = true;
        let roots: Vec<PathBuf> = self
            .workspace
            .read(cx)
            .projects
            .iter()
            .map(|open| open.path.clone())
            .collect();
        self.search.task = Some(cx.spawn(async move |this, cx| {
            // A keystroke landing in the meantime drops this task, so a burst
            // of typing is one search.
            cx.background_executor().timer(SETTLE).await;
            let found = cx
                .background_executor()
                .spawn(async move { search_all(&roots, &query) })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.search.hits = this.ranked_hits(found, cx);
                this.search.limit = SHOWN;
                this.search.searching = false;
                this.search.selected = 0;
                cx.notify();
            });
        }));
        cx.notify();
    }

    /// What was touched last, across every open project.
    fn recent_hits(&self, cx: &App) -> Vec<Hit> {
        self.recent_rows(cx)
            .into_iter()
            .map(|row| Hit { row, snippet: None })
            .collect()
    }

    /// Title matches first, then the rest, each most recently touched first.
    fn ranked_hits(&self, found: Vec<Found>, cx: &App) -> Vec<Hit> {
        let workspace = self.workspace.read(cx);
        let mut hits: Vec<(bool, u128, Hit)> = found
            .into_iter()
            .filter_map(|found| {
                let project = workspace
                    .projects
                    .iter()
                    .position(|open| open.path == found.root)?;
                let open = &workspace.projects[project];
                let (row, touched) = match found.kind {
                    Kind::Article => {
                        let ix = open.articles.iter().position(|a| a.id == found.id)?;
                        (Row::Article { project, ix }, open.articles[ix].touched)
                    }
                    Kind::Board => {
                        let ix = open.boards.iter().position(|b| b.id == found.id)?;
                        (Row::Board { project, ix }, open.boards[ix].touched)
                    }
                    Kind::Session => {
                        let chat = open
                            .sessions
                            .iter()
                            .find(|chat| chat.record.as_deref() == Some(found.id.as_str()))?;
                        (
                            Row::Session {
                                project,
                                id: chat.id,
                            },
                            chat.touched(),
                        )
                    }
                };
                Some((
                    found.in_title,
                    touched,
                    Hit {
                        row,
                        snippet: found.snippet,
                    },
                ))
            })
            .collect();
        hits.sort_by(|a, b| b.0.cmp(&a.0).then(b.1.cmp(&a.1)));
        hits.into_iter().map(|(.., hit)| hit).collect()
    }

    fn select_next(&mut self, _: &SelectNext, _: &mut Window, cx: &mut Context<Self>) {
        self.step_hit(1, cx);
    }

    fn select_prev(&mut self, _: &SelectPrev, _: &mut Window, cx: &mut Context<Self>) {
        self.step_hit(-1, cx);
    }

    fn step_hit(&mut self, by: isize, cx: &mut Context<Self>) {
        let len = self.search.shown().len();
        if len == 0 {
            return;
        }
        let at = (self.search.selected as isize + by).rem_euclid(len as isize) as usize;
        self.search.selected = at;
        self.search.scroll.scroll_to_item(at);
        cx.notify();
    }

    fn open_selected(&mut self, _: &OpenHit, window: &mut Window, cx: &mut Context<Self>) {
        self.open_hit(self.search.selected, window, cx);
    }

    /// Open a hit, and mark the query in it where it was found in the body.
    fn open_hit(&mut self, ix: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(hit) = self.search.shown().get(ix).copied().cloned() else {
            return;
        };
        let query = self.search.field.read(cx).content().clone();
        self.dismiss_search(&DismissSearch, window, cx);
        self.open_row(hit.row, window, cx);
        if hit.snippet.is_some() {
            let field = self.leaf().find_field.clone();
            self.leaf_mut().finding = true;
            field.update(cx, |field, cx| field.set_content(query, cx));
        }
        cx.notify();
    }

    /// The sidebar's row that raises the palette.
    pub(crate) fn search_row(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = Theme::of(cx).clone();
        let chord = keymap::label(Command::Search, &self.workspace.read(cx).settings.shortcuts);
        sidebar::row("search-row", "search-row", false, 0, &theme)
            .flex_none()
            .mb(px(4.))
            .child(
                icons::icon(icons::text::Search)
                    .size(px(14.))
                    .text_color(theme.text_muted),
            )
            .child(
                div()
                    .flex_1()
                    .text_style(TextStyle::Body)
                    .text_color(theme.text_muted)
                    .child("Search"),
            )
            .children(chord.map(|chord| popover::kbd_hint(&theme, chord)))
            .on_click(
                cx.listener(|this, _, window, cx| this.toggle_search(&ToggleSearch, window, cx)),
            )
            .into_any_element()
    }

    /// The palette over the window, while it is up.
    pub(crate) fn search_palette(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.search.open {
            return None;
        }
        let theme = Theme::of(cx).clone();
        let empty = self.search.query(cx).is_none();
        let rows: Vec<AnyElement> = self
            .search
            .shown()
            .into_iter()
            .enumerate()
            .map(|(ix, hit)| self.hit_row(ix, hit, cx))
            .collect();
        let note = match (rows.is_empty(), self.search.searching, empty) {
            (false, ..) => None,
            (true, true, _) => Some("Searching…"),
            (true, false, true) => Some("Nothing yet"),
            (true, false, false) => Some("No matches"),
        };
        Some(
            div()
                .id("search-scrim")
                // Whatever is under the scrim takes no pointer, the wheel
                // included.
                .occlude()
                .absolute()
                .inset_0()
                .flex()
                .flex_col()
                .items_center()
                .pt(px(96.))
                .bg(theme.scrim())
                .on_click(cx.listener(|this, _, window, cx| {
                    this.dismiss_search(&DismissSearch, window, cx)
                }))
                .child(
                    div()
                        .id("search-palette")
                        .w(px(560.))
                        .max_h(px(440.))
                        .flex()
                        .flex_col()
                        .rounded(px(Theme::panel_radius()))
                        .border_1()
                        .border_color(theme.border)
                        .bg(theme.surface)
                        .overflow_hidden()
                        .on_click(|_, _, cx| cx.stop_propagation())
                        .on_action(cx.listener(Self::dismiss_search))
                        .on_action(cx.listener(Self::select_next))
                        .on_action(cx.listener(Self::select_prev))
                        .on_action(cx.listener(Self::open_selected))
                        .on_action(cx.listener(Self::next_filter))
                        .on_action(cx.listener(Self::prev_filter))
                        .child(
                            div()
                                .flex_none()
                                .flex()
                                .flex_row()
                                .items_center()
                                .gap(px(8.))
                                .px(px(14.))
                                .py(px(10.))
                                .border_b_1()
                                .border_color(theme.border)
                                .child(
                                    icons::icon(icons::text::Search)
                                        .size(px(16.))
                                        .text_color(theme.text_faint),
                                )
                                .child(div().flex_1().min_w_0().child(self.search.field.clone())),
                        )
                        .children(empty.then(|| {
                            div()
                                .flex_none()
                                .px(px(14.))
                                .pt(px(8.))
                                .pb(px(2.))
                                .text_style(TextStyle::Subheadline)
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(theme.text_faint)
                                .child("Recent")
                        }))
                        .child(
                            div()
                                .id("search-hits")
                                .flex_1()
                                .min_h_0()
                                .overflow_y_scroll()
                                .track_scroll(&self.search.scroll)
                                .p(px(6.))
                                .flex()
                                .flex_col()
                                .children(rows)
                                .children(note.map(|note| {
                                    div()
                                        .px(px(8.))
                                        .py(px(10.))
                                        .text_style(TextStyle::Subheadline)
                                        .text_color(theme.text_faint)
                                        .child(note)
                                })),
                        )
                        .child(self.palette_footer(cx)),
                )
                .into_any_element(),
        )
    }

    fn next_filter(&mut self, _: &NextFilter, _: &mut Window, cx: &mut Context<Self>) {
        self.step_filter(1, cx);
    }

    fn prev_filter(&mut self, _: &PrevFilter, _: &mut Window, cx: &mut Context<Self>) {
        self.step_filter(-1, cx);
    }

    fn step_filter(&mut self, by: isize, cx: &mut Context<Self>) {
        let at = FILTERS
            .iter()
            .position(|(kind, _)| *kind == self.search.filter)
            .unwrap_or(0) as isize;
        let at = (at + by).rem_euclid(FILTERS.len() as isize) as usize;
        self.set_filter(FILTERS[at].0, cx);
    }

    fn set_filter(&mut self, filter: Option<Kind>, cx: &mut Context<Self>) {
        self.search.filter = filter;
        self.search.selected = 0;
        self.search.scroll.scroll_to_item(0);
        cx.notify();
    }

    /// The foot of the palette: the kinds to narrow to, and the keys.
    fn palette_footer(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = Theme::of(cx).clone();
        let hint = |keys: &'static str, what: &'static str| {
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap(px(4.))
                .child(popover::kbd_hint(&theme, keys))
                .child(what)
        };
        div()
            .flex_none()
            .flex()
            .flex_row()
            .items_center()
            .justify_between()
            .px(px(10.))
            .py(px(6.))
            .border_t_1()
            .border_color(theme.border)
            .text_style(TextStyle::Subheadline)
            .text_color(theme.text_faint)
            .child(self.filter_chips(cx))
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(10.))
                    .child(hint("⇥", "Filter"))
                    .child(hint("↵", "Open")),
            )
            .into_any_element()
    }

    /// The kinds to narrow to.
    fn filter_chips(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = Theme::of(cx).clone();
        div()
            .flex()
            .flex_row()
            .gap(px(4.))
            .children(FILTERS.iter().enumerate().map(|(ix, (kind, label))| {
                let on = *kind == self.search.filter;
                let kind = *kind;
                div()
                    .id(("search-filter", ix))
                    .px(px(8.))
                    .py(px(2.))
                    .rounded_full()
                    .border_1()
                    .cursor_pointer()
                    .text_style(TextStyle::Subheadline)
                    .when(on, |chip| {
                        chip.bg(theme.element_active)
                            .border_color(theme.border)
                            .text_color(theme.text)
                    })
                    .when(!on, |chip| {
                        chip.border_color(gpui::transparent_black())
                            .text_color(theme.text_muted)
                            .hover(|chip| chip.bg(theme.element_hover))
                    })
                    .child(*label)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        cx.stop_propagation();
                        this.set_filter(kind, cx);
                    }))
            }))
            .into_any_element()
    }

    fn hit_row(&self, ix: usize, hit: &Hit, cx: &mut Context<Self>) -> AnyElement {
        let theme = Theme::of(cx).clone();
        let project = match hit.row {
            Row::Session { project, .. }
            | Row::Board { project, .. }
            | Row::Article { project, .. } => self
                .workspace
                .read(cx)
                .projects
                .get(project)
                .map(|open| open.name()),
            _ => None,
        };
        let icon: Icon = match hit.row {
            Row::Session { .. } => icons::social::MessageCircle.into(),
            Row::Board { .. } => icons::development::SquareKanban.into(),
            _ => icons::files::FileText.into(),
        };
        let title = self.label_of_row(hit.row, cx);
        let selected = ix == self.search.selected;
        let snippet = hit.snippet.as_ref().map(|(line, at)| {
            let (text, at) = clipped(line, at.clone());
            StyledText::new(text).with_highlights([(
                at,
                HighlightStyle {
                    color: Some(theme.text),
                    background_color: Some(theme.accent.opacity(0.22)),
                    ..Default::default()
                },
            )])
        });
        div()
            .id(("search-hit", ix))
            .flex()
            .flex_row()
            .items_start()
            .gap(px(10.))
            .px(px(8.))
            .py(px(6.))
            .rounded(px(Theme::control_radius()))
            .cursor_pointer()
            .when(selected, |el| el.bg(theme.element_active))
            .when(!selected, |el| el.hover(|el| el.bg(theme.element_hover)))
            .child(
                div()
                    .pt(px(2.))
                    .child(icons::icon(icon).size(px(14.)).text_color(theme.text_muted)),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap(px(2.))
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap(px(8.))
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .truncate()
                                    .text_style(TextStyle::Body)
                                    .text_color(theme.text)
                                    .child(title),
                            )
                            .children(project.map(|project| {
                                div()
                                    .flex_none()
                                    .text_style(TextStyle::Subheadline)
                                    .text_color(theme.text_faint)
                                    .child(project)
                            })),
                    )
                    .children(snippet.map(|snippet| {
                        div()
                            .truncate()
                            .text_style(TextStyle::Subheadline)
                            .text_color(theme.text_muted)
                            .child(snippet)
                    })),
            )
            .on_click(cx.listener(move |this, _, window, cx| {
                cx.stop_propagation();
                this.open_hit(ix, window, cx);
            }))
            .into_any_element()
    }
}

/// The line cut to start a little ahead of the match and trimmed, and the
/// match's range in what is left.
fn clipped(line: &str, at: Range<usize>) -> (SharedString, Range<usize>) {
    let start = line[..at.start]
        .char_indices()
        .rev()
        .nth(LEAD)
        .map_or(0, |(ix, _)| ix);
    let head = &line[start..];
    let trimmed = head.trim_start();
    let cut = start + (head.len() - trimmed.len());
    let lead = if start > 0 { "…" } else { "" };
    let text = format!("{lead}{}", trimmed.trim_end());
    let from = (lead.len() + at.start.saturating_sub(cut)).min(text.len());
    let to = (lead.len() + at.end.saturating_sub(cut)).min(text.len());
    (text.into(), from..to)
}

/// Every entry in `roots` holding `query`, each once with its best match: the
/// title where it matched there, else its earliest block.
fn search_all(roots: &[PathBuf], query: &Query) -> Vec<Found> {
    let kinds = [Kind::Article, Kind::Board, Kind::Session];
    let mut found = Vec::new();
    for root in roots {
        let (tx, rx) = mpsc::channel();
        search::disk(&fs::Project::new(root), &kinds, query, &tx);
        drop(tx);
        let mut best: HashMap<(Kind, String), search::Match> = HashMap::new();
        for hit in rx {
            let key = (hit.item.kind, hit.item.id.clone());
            if best
                .get(&key)
                .is_none_or(|held| rank(&hit.block) < rank(&held.block))
            {
                best.insert(key, hit);
            }
        }
        found.extend(best.into_iter().map(|((kind, id), hit)| {
            let in_title = hit.block == Block::Title;
            Found {
                root: root.clone(),
                kind,
                id,
                in_title,
                snippet: (!in_title).then_some((hit.line, hit.column)),
            }
        }));
    }
    found
}

/// The filters in the order the chips show and `tab` steps them.
const FILTERS: [(Option<Kind>, &str); 4] = [
    (None, "All"),
    (Some(Kind::Session), "Sessions"),
    (Some(Kind::Board), "Boards"),
    (Some(Kind::Article), "Articles"),
];

fn kind_of(row: Row) -> Option<Kind> {
    match row {
        Row::Session { .. } => Some(Kind::Session),
        Row::Board { .. } => Some(Kind::Board),
        Row::Article { .. } => Some(Kind::Article),
        _ => None,
    }
}

/// Which of two matches in one entry is shown: the title, then the earliest
/// block.
fn rank(block: &Block) -> (u8, usize) {
    match block {
        Block::Title => (0, 0),
        Block::Line(ix) | Block::Chat(ix) => (1, *ix),
        Block::Card(_) => (1, usize::MAX),
    }
}

#[cfg(test)]
#[path = "../../tests/unit/search_palette.rs"]
mod tests;
