//! The search palette: every open project's articles, boards and sessions,
//! raised from the sidebar's Search row. Command-Enter applies its draft to
//! the workspace until the sidebar's clear button is pressed.
//!
//! A query also lists the menu bar's commands, matched by name or by the chord
//! bound to them; an empty one lists the commands run last. A query starting
//! with `>` lists commands alone.
//!
//! An empty query lists what was touched last. A query is searched off disk on
//! a background thread — see [`artifact::search::disk`] — so an entry's hits
//! are as of its last write. Opening a hit in an entry's body puts the query in
//! its pane's find bar — see [`crate::view::find`].

use crate::view::{
    keymap::{self, Command},
    menubar,
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
        widgets::Buttons as _,
    },
};
use std::{collections::HashMap, ops::Range, path::PathBuf, sync::mpsc, time::Duration};

actions!(
    cydonia_search,
    [
        ToggleSearch,
        ApplySearch,
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

/// Commands an empty query lists outside the Commands filter.
const RECENT_COMMANDS: usize = 5;

/// What a query starting with this lists: commands alone.
const COMMAND_PREFIX: char = '>';

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
        KeyBinding::new("secondary-enter", ApplySearch, ctx),
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

/// A row as the palette lists it: a command, then the entries.
#[derive(Clone, Copy)]
enum Pick<'a> {
    Command(&'a menubar::Command),
    Hit(&'a Hit),
}

/// What the palette is narrowed to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Filter {
    Kind(Kind),
    Commands,
}

/// An entry's best match as the search found it.
#[derive(Clone)]
struct Found {
    root: PathBuf,
    kind: Kind,
    id: String,
    in_title: bool,
    snippet: Option<(String, Range<usize>)>,
}

struct Applied {
    query: Query,
    filter: Option<Kind>,
    found: Vec<Found>,
    ready: bool,
}

pub(crate) struct Search {
    applied: Option<Applied>,
    applied_task: Option<Task<()>>,
    open: bool,
    field: Entity<TextField>,
    /// The commands the focused surface could run when the palette opened.
    commands: Vec<menubar::Command>,
    /// Which of [`Self::commands`] the field's query matches, in order.
    matched: Vec<usize>,
    /// What the last finished listing found. Kept on screen while the next
    /// one runs, so typing does not empty the palette between keystrokes.
    hits: Vec<Hit>,
    /// Whether a search for the field's query is still running.
    searching: bool,
    /// The one kind listed, or every kind.
    filter: Option<Filter>,
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
                .with_placeholder("Search articles, boards, sessions and commands…")
        });
        cx.subscribe(&field, |this, _, event: &FieldEvent, cx| {
            if matches!(event, FieldEvent::Changed(_)) {
                this.refresh_search(cx);
            }
        })
        .detach();
        Self {
            applied: None,
            applied_task: None,
            open: false,
            field,
            commands: Vec::new(),
            matched: Vec::new(),
            hits: Vec::new(),
            searching: false,
            filter: None,
            limit: RECENT,
            selected: 0,
            scroll: ScrollHandle::new(),
            task: None,
        }
    }

    /// The rows the filter leaves, in order: commands, then the entries.
    fn shown(&self, cx: &App) -> Vec<Pick<'_>> {
        let only_commands = self.only_commands(cx);
        let commands = self
            .matched
            .iter()
            .filter(|_| self.filter.is_none() || only_commands)
            .map(|&ix| Pick::Command(&self.commands[ix]));
        let hits = self
            .hits
            .iter()
            .filter(|_| !only_commands)
            .filter(|hit| match self.filter {
                Some(Filter::Kind(kind)) => kind_of(hit.row) == Some(kind),
                _ => true,
            })
            .take(self.limit)
            .map(Pick::Hit);
        commands.chain(hits).collect()
    }

    /// Whether the palette lists commands and nothing else: the Commands
    /// filter, or a query starting with [`COMMAND_PREFIX`].
    fn only_commands(&self, cx: &App) -> bool {
        self.filter == Some(Filter::Commands)
            || self
                .field
                .read(cx)
                .content()
                .trim_start()
                .starts_with(COMMAND_PREFIX)
    }

    /// Match the commands against what the field holds, less the prefix. An
    /// empty query lists the ones run last, and every other one after them
    /// under the Commands filter.
    fn match_commands(&mut self, recent: &[String], cx: &App) {
        let only = self.only_commands(cx);
        let content = self.field.read(cx).content();
        let text = content
            .trim()
            .trim_start_matches(COMMAND_PREFIX)
            .trim()
            .to_lowercase();
        if text.is_empty() {
            let keys: Vec<String> = self.commands.iter().map(menubar::Command::key).collect();
            let ran = recent
                .iter()
                .filter_map(|key| keys.iter().position(|held| held == key));
            self.matched = match only {
                true => {
                    let ran: Vec<usize> = ran.collect();
                    let rest = (0..self.commands.len()).filter(|ix| !ran.contains(ix));
                    ran.iter().copied().chain(rest).collect()
                }
                false => ran.take(RECENT_COMMANDS).collect(),
            };
            return;
        }
        let keys = menubar::query_keys(&text);
        self.matched = self
            .commands
            .iter()
            .enumerate()
            .filter(|(_, command)| {
                command.name.to_lowercase().contains(&text)
                    || keys.is_some() && command.keys == keys
            })
            .map(|(ix, _)| ix)
            .collect();
    }

    /// What the entries are searched for. Nothing for a query of commands.
    fn query(&self, cx: &App) -> Option<Query> {
        let content = self.field.read(cx).content();
        match content.trim_start().starts_with(COMMAND_PREFIX) {
            true => None,
            false => Query::literal(content),
        }
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
        // Read before the field takes focus: what can run is what the
        // surface under the palette could.
        self.search.commands = menubar::commands(window, cx);
        let query = self
            .search
            .applied
            .as_ref()
            .map(|a| a.query.text().to_owned());
        self.search.filter = self
            .search
            .applied
            .as_ref()
            .and_then(|a| a.filter)
            .map(Filter::Kind);
        self.search.field.update(cx, |field, cx| {
            field.set_content(query.unwrap_or_default(), cx);
        });
        self.refresh_search(cx);
        window.focus(&self.search.field.read(cx).focus_handle(cx), cx);
        cx.notify();
    }

    fn apply_search(&mut self, _: &ApplySearch, window: &mut Window, cx: &mut Context<Self>) {
        let Some(query) = self.search.query(cx) else {
            return;
        };
        self.search.applied = Some(Applied {
            query,
            filter: match self.search.filter {
                Some(Filter::Kind(kind)) => Some(kind),
                _ => None,
            },
            found: Vec::new(),
            ready: false,
        });
        for leaf in &mut self.leaves {
            leaf.finding = false;
            leaf.find_at = 0;
        }
        self.refresh_applied_search(cx);
        self.dismiss_search(&DismissSearch, window, cx);
    }

    pub(crate) fn refresh_applied_search(&mut self, cx: &mut Context<Self>) {
        let Some(applied) = self.search.applied.as_mut() else {
            return;
        };
        let query = applied.query.clone();
        let roots = self
            .workspace
            .read(cx)
            .projects
            .iter()
            .map(|p| p.path.clone())
            .collect::<Vec<_>>();
        self.search.applied_task = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(SETTLE).await;
            let found = cx
                .background_executor()
                .spawn(async move { search_all(&roots, &query) })
                .await;
            let _ = this.update(cx, |this, cx| {
                if let Some(applied) = this.search.applied.as_mut() {
                    applied.found = found;
                    applied.ready = true;
                }
                cx.notify();
            });
        }));
    }

    pub(crate) fn applied_query(&self) -> Option<&Query> {
        self.search.applied.as_ref().map(|a| &a.query)
    }

    pub(crate) fn applied_rows(&self, cx: &App) -> Option<Vec<Row>> {
        let applied = self.search.applied.as_ref()?;
        Some(
            self.ranked_hits(applied.found.clone(), cx)
                .into_iter()
                .filter(|hit| {
                    applied
                        .filter
                        .is_none_or(|kind| kind_of(hit.row) == Some(kind))
                })
                .map(|hit| hit.row)
                .collect(),
        )
    }

    pub(crate) fn clear_applied_search(&mut self, cx: &mut Context<Self>) {
        self.search.applied = None;
        self.search.applied_task = None;
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
        self.match_commands(cx);
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

    fn match_commands(&mut self, cx: &mut Context<Self>) {
        let recent = self.workspace.read(cx).recent_commands().to_vec();
        self.search.match_commands(&recent, cx);
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
        let len = self.search.shown(cx).len();
        if len == 0 {
            return;
        }
        let at = (self.search.selected as isize + by).rem_euclid(len as isize) as usize;
        self.search.selected = at;
        // The list's children count a heading before each section.
        let commands = self
            .search
            .shown(cx)
            .iter()
            .filter(|pick| matches!(pick, Pick::Command(_)))
            .count();
        let headings = match at < commands {
            true => 1,
            false => usize::from(commands > 0) + 1,
        };
        self.search.scroll.scroll_to_item(at + headings);
        cx.notify();
    }

    fn open_selected(&mut self, _: &OpenHit, window: &mut Window, cx: &mut Context<Self>) {
        self.open_hit(self.search.selected, window, cx);
    }

    /// Open a hit, and mark the query in it where it was found in the body. A
    /// command runs on the surface the palette was raised over.
    fn open_hit(&mut self, ix: usize, window: &mut Window, cx: &mut Context<Self>) {
        let hit = match self.search.shown(cx).get(ix).copied() {
            Some(Pick::Hit(hit)) => hit.clone(),
            Some(Pick::Command(command)) => {
                let action = command.action.boxed_clone();
                let key = command.key();
                self.workspace
                    .update(cx, |workspace, _| workspace.ran_command(key));
                self.dismiss_search(&DismissSearch, window, cx);
                window.dispatch_action(action, cx);
                return;
            }
            None => return,
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
        if let Some(applied) = &self.search.applied {
            let label = match applied.filter {
                Some(kind) => format!(
                    "{} · {}",
                    applied.query.text(),
                    FILTERS
                        .iter()
                        .find(|(k, _)| *k == Some(Filter::Kind(kind)))
                        .unwrap()
                        .1
                ),
                None => applied.query.text().to_owned(),
            };
            let count = self.applied_rows(cx).map_or(0, |rows| rows.len());
            return sidebar::row("applied-search", "applied-search", false, false, 0, &theme)
                .flex_none()
                .min_w_0()
                .mb(px(4.))
                .child(
                    icons::icon(icons::text::Search)
                        .size(px(14.))
                        .text_color(theme.text_muted),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .text_style(TextStyle::Body)
                        .text_color(theme.text)
                        .child(label),
                )
                .child(
                    div()
                        .flex_none()
                        .text_style(TextStyle::Caption)
                        .text_color(theme.text_muted)
                        .child(if !applied.ready {
                            "…".to_owned()
                        } else {
                            count.to_string()
                        }),
                )
                .child(
                    theme
                        .ghost("clear-workspace-search")
                        .debug_selector(|| "clear-workspace-search".into())
                        .flex_none()
                        .size(px(24.))
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(
                            icons::icon(icons::notifications::X)
                                .size(px(14.))
                                .text_color(theme.text_muted),
                        )
                        .on_click(cx.listener(|this, _, _, cx| {
                            cx.stop_propagation();
                            this.clear_applied_search(cx);
                        })),
                )
                .on_click(
                    cx.listener(|this, _, window, cx| {
                        this.toggle_search(&ToggleSearch, window, cx)
                    }),
                )
                .into_any_element();
        }
        sidebar::row("search-row", "search-row", false, false, 0, &theme)
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
        let heading = |title: &'static str| {
            div()
                .px(px(8.))
                .pt(px(6.))
                .pb(px(4.))
                .text_style(TextStyle::Subheadline)
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(theme.text_faint)
                .child(title)
                .into_any_element()
        };
        let mut rows: Vec<AnyElement> = Vec::new();
        let mut section = None;
        for (ix, pick) in self.search.shown(cx).into_iter().enumerate() {
            let title = match (pick, empty) {
                (Pick::Command(_), _) => "Commands",
                (Pick::Hit(_), true) => "Recent",
                (Pick::Hit(_), false) => "Entries",
            };
            if section != Some(title) {
                section = Some(title);
                rows.push(heading(title));
            }
            rows.push(match pick {
                Pick::Command(command) => self.command_row(ix, command, cx),
                Pick::Hit(hit) => self.hit_row(ix, hit, cx),
            });
        }
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
                        .on_action(cx.listener(Self::apply_search))
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
                        .child(self.palette_footer(cx))
                        .child(
                            div()
                                .id("apply-workspace-search")
                                .px(px(14.))
                                .py(px(8.))
                                .border_t_1()
                                .border_color(theme.border)
                                .text_style(TextStyle::Subheadline)
                                .text_color(if empty { theme.text_faint } else { theme.text })
                                .when(!empty, |el| {
                                    el.cursor_pointer().hover(|el| el.bg(theme.element_hover))
                                })
                                .child(keymap::platform(
                                    "Apply to workspace  ⌘↵",
                                    "Apply to workspace  Ctrl+Enter",
                                ))
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.apply_search(&ApplySearch, window, cx)
                                })),
                        ),
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

    fn set_filter(&mut self, filter: Option<Filter>, cx: &mut Context<Self>) {
        self.search.filter = filter;
        self.match_commands(cx);
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

    fn command_row(
        &self,
        ix: usize,
        command: &menubar::Command,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = Theme::of(cx).clone();
        let selected = ix == self.search.selected;
        div()
            .id(("search-hit", ix))
            .flex()
            .flex_row()
            .items_center()
            .gap(px(10.))
            .px(px(8.))
            .py(px(6.))
            .rounded(px(Theme::control_radius()))
            .cursor_pointer()
            .when(selected, |el| el.bg(theme.element_active))
            .when(!selected, |el| el.hover(|el| el.bg(theme.element_hover)))
            .child(
                icons::icon(icons::development::SquareTerminal)
                    .size(px(14.))
                    .text_color(theme.text_muted),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_style(TextStyle::Body)
                    .text_color(theme.text)
                    .child(command.name.clone()),
            )
            .child(
                div()
                    .flex_none()
                    .text_style(TextStyle::Subheadline)
                    .text_color(theme.text_faint)
                    .child(command.menu.clone()),
            )
            .children(
                command
                    .shortcut
                    .clone()
                    .map(|chord| popover::kbd_hint(&theme, chord)),
            )
            .on_click(cx.listener(move |this, _, window, cx| {
                cx.stop_propagation();
                this.open_hit(ix, window, cx);
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
const FILTERS: [(Option<Filter>, &str); 5] = [
    (None, "All"),
    (Some(Filter::Commands), "Commands"),
    (Some(Filter::Kind(Kind::Session)), "Sessions"),
    (Some(Filter::Kind(Kind::Board)), "Boards"),
    (Some(Filter::Kind(Kind::Article)), "Articles"),
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
