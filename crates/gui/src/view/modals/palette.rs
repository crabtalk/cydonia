//! The search palette: every open project's articles, boards and sessions,
//! raised from the sidebar's Search row. Command-Enter applies its draft to
//! the workspace — see [`crate::view::search`].
//!
//! A query also lists the menu bar's commands, matched by name or by the chord
//! bound to them; an empty one lists the commands run last. A query starting
//! with `>` lists commands alone, and one starting with a kind's prefix,
//! `s:`, `a:` or `b:`, that kind's entries alone. A label's prefix, `l:` and
//! the label, after it or alone, lists the entries carrying that label.
//!
//! A query lists the cards and entries `@` would name for it first — see
//! [`mention::Query::rank_named`] — then the ones it is found in. A card opens in the drawer, as its chip does.
//!
//! An empty query lists what was touched last. A query is searched off disk on
//! a background thread — see [`artifact::search::disk`] — so an entry's hits
//! are as of its last write. Opening a hit in an entry's body puts the query in
//! its pane's find bar — see [`crate::view::pane::find`].

use crate::{
    model::workspace::{Opening, Showing, Workspace},
    view::{
        keymap,
        kit::{
            icon::kind_icon,
            link::mention::{self, Linkables},
            text::CardTitle,
        },
        search::{Found, SETTLE, search_all},
        sidebar::Row,
        window::{menubar, root::Cydonia},
    },
};
use artifact::{kind::Kind, search::Query};
use bezel::{
    gpui::{
        self, AnyElement, App, Context, DismissEvent, Entity, EventEmitter, FocusHandle, Focusable,
        FontWeight, HighlightStyle, KeyBinding, ScrollHandle, SharedString, StyledText, Task,
        WeakEntity, Window, actions, div, prelude::*, px,
    },
    theme::{TextStyle, Theme, Typeset as _},
    ui::{
        icons,
        input::{FieldEvent, TextField},
        popover,
    },
};
use std::{ops::Range, path::PathBuf, rc::Rc};

const CONTEXT: &str = "CydoniaSearch";

/// Rows an empty query lists.
const RECENT: usize = 12;

/// Commands an empty query lists outside the Commands filter.
const RECENT_COMMANDS: usize = 5;

/// What a query starting with this lists: commands alone.
const COMMAND_PREFIX: char = '>';

/// Rows a query lists, at most.
pub(crate) const SHOWN: usize = 50;

/// Characters of a snippet kept ahead of the match.
const LEAD: usize = 32;

const PLACEHOLDER: &str = "Search articles, boards, sessions and commands…";

const LINK_PLACEHOLDER: &str = "Link a session…";

/// The filters in the order the chips show and `tab` steps them.
const FILTERS: [(Option<Filter>, &str); 5] = [
    (None, "All"),
    (Some(Filter::Commands), "Commands"),
    (Some(Filter::Kind(Kind::Session)), "Sessions"),
    (Some(Filter::Kind(Kind::Board)), "Boards"),
    (Some(Filter::Kind(Kind::Article)), "Articles"),
];

actions!(
    cydonia_search,
    [
        ToggleSearch,
        NewTab,
        ApplySearch,
        SelectNext,
        SelectPrev,
        OpenHit,
        NextFilter,
        PrevFilter
    ]
);

pub fn bindings() -> Vec<KeyBinding> {
    let ctx = Some(CONTEXT);
    vec![
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

/// A session the palette handed back while it was linking, as its link,
/// `cydonia://project#43`.
pub(crate) struct Linked {
    pub url: String,
}

/// What picking an entry does while the palette links rather than opens.
pub(crate) type OnLink = Rc<dyn Fn(&mut Cydonia, Linked, &mut Window, &mut Context<Cydonia>)>;

/// One row of the palette.
#[derive(Clone)]
pub(crate) struct Hit {
    pub(crate) row: Row,
    /// Where the query was found, when it was found in the entry's body: the
    /// line holding it, and where in the line.
    snippet: Option<(String, Range<usize>)>,
    /// A card on the board `row` names.
    card: Option<HitCard>,
}

#[derive(Clone)]
struct HitCard {
    /// `project#DEV-12`.
    reference: String,
    /// `DEV-12`.
    handle: String,
    title: String,
}

/// A row as the palette lists it: a command, then the entries.
#[derive(Clone, Copy)]
enum Pick<'a> {
    Command(&'a menubar::Command),
    Hit(&'a Hit),
}

/// What the palette is narrowed to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Filter {
    Kind(Kind),
    Commands,
}

/// The palette, as a modal.
pub(crate) struct Palette {
    field: Entity<TextField>,
    /// The commands the focused surface could run when the palette opened.
    commands: Vec<menubar::Command>,
    /// Which of [`Self::commands`] the field's query matches, in order.
    matched: Vec<usize>,
    /// What was touched last when the palette opened, listed for an empty
    /// query.
    recent: Vec<Row>,
    /// What the last finished listing found. Kept on screen while the next
    /// one runs, so typing does not empty the palette between keystrokes.
    hits: Vec<Hit>,
    /// Whether a search for the field's query is still running.
    searching: bool,
    /// The one kind listed, or every kind.
    filter: Option<Filter>,
    /// Rows listed at most, once filtered.
    limit: usize,
    /// Set while the palette links an entry: what picking one does in place
    /// of opening it. It lists no commands.
    linking: Option<OnLink>,
    /// Whether what is picked opens as a tab of its own rather than in the
    /// preview's place — see [`Cydonia::new_tab`].
    keep: bool,
    selected: usize,
    scroll: ScrollHandle,
    task: Option<Task<()>>,
    /// What held focus when the palette opened. A command runs from it.
    returns_to: Option<FocusHandle>,
    workspace: Entity<Workspace>,
    root: WeakEntity<Cydonia>,
}

impl Filter {
    /// What the filter is called on its chip.
    pub(crate) fn label(self) -> &'static str {
        FILTERS
            .iter()
            .find(|(filter, _)| *filter == Some(self))
            .map_or("", |(_, label)| label)
    }
}

impl Hit {
    /// What a search found, as rows: title matches first, then the rest, each
    /// most recently touched first.
    pub(crate) fn ranked(found: Vec<Found>, workspace: &Workspace, cx: &App) -> Vec<Hit> {
        let mut hits: Vec<(bool, u128, Hit)> = found
            .into_iter()
            .filter_map(|found| {
                let open = workspace
                    .projects
                    .iter()
                    .find(|open| open.path == found.root)?;
                let showing = open.stored(found.kind, &found.id, cx)?;
                let touched = open.facts(&showing, cx)?.touched;
                let row = Row::Entry {
                    project: open.id,
                    showing,
                };
                Some((
                    found.in_title,
                    touched,
                    Hit {
                        row,
                        snippet: found.snippet,
                        card: None,
                    },
                ))
            })
            .collect();
        hits.sort_by(|a, b| b.0.cmp(&a.0).then(b.1.cmp(&a.1)));
        hits.into_iter().map(|(.., hit)| hit).collect()
    }
}

impl Cydonia {
    pub(crate) fn toggle_search(
        &mut self,
        _: &ToggleSearch,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // Read before the field takes focus: what can run is what the
        // surface under the palette could.
        let commands = menubar::commands(window, cx);
        let (query, filter) = self.search.read(cx).draft();
        let recent = self.list(cx).recent_rows();
        let workspace = self.workspace.clone();
        let root = cx.entity().downgrade();
        self.toggle_modal(window, cx, |window, cx| {
            let mut palette = Palette::new(workspace, root, recent, window, cx);
            palette.commands = commands;
            palette.filter = filter.map(Filter::Kind);
            palette
                .field
                .update(cx, |field, cx| field.set_content(query, cx));
            palette.refresh(cx);
            palette
        });
    }

    /// Raise the palette to open an entry in a tab of its own.
    pub(crate) fn new_tab(&mut self, _: &NewTab, window: &mut Window, cx: &mut Context<Self>) {
        let recent = self.list(cx).recent_rows();
        let workspace = self.workspace.clone();
        let root = cx.entity().downgrade();
        self.toggle_modal(window, cx, |window, cx| {
            let mut palette = Palette::new(workspace, root, recent, window, cx);
            palette.keep = true;
            palette.refresh(cx);
            palette
        });
    }

    /// Raise the palette to link an entry: Enter hands the one picked to `on`
    /// rather than opening it.
    pub(crate) fn link_search(&mut self, on: OnLink, window: &mut Window, cx: &mut Context<Self>) {
        let recent = self.list(cx).recent_rows();
        let workspace = self.workspace.clone();
        let root = cx.entity().downgrade();
        self.toggle_modal(window, cx, |window, cx| {
            let mut palette = Palette::new(workspace, root, recent, window, cx);
            palette.linking = Some(on);
            palette.filter = Some(Filter::Kind(Kind::Session));
            palette
                .field
                .update(cx, |field, cx| field.set_placeholder(LINK_PLACEHOLDER, cx));
            palette.refresh(cx);
            palette
        });
    }
}

impl Palette {
    fn new(
        workspace: Entity<Workspace>,
        root: WeakEntity<Cydonia>,
        recent: Vec<Row>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let field = cx.new(|cx| {
            TextField::new(cx)
                .with_frame(false)
                .with_key_context(CONTEXT)
                .with_placeholder(PLACEHOLDER)
        });
        cx.subscribe(&field, |this, _, event: &FieldEvent, cx| {
            if matches!(event, FieldEvent::Changed(_)) {
                this.refresh(cx);
            }
        })
        .detach();
        Self {
            field,
            commands: Vec::new(),
            matched: Vec::new(),
            recent,
            hits: Vec::new(),
            searching: false,
            filter: None,
            limit: RECENT,
            linking: None,
            keep: false,
            selected: 0,
            scroll: ScrollHandle::new(),
            task: None,
            returns_to: window.focused(cx),
            workspace,
            root,
        }
    }

    /// The rows the filter leaves, in order: commands, then the entries.
    fn shown(&self, cx: &App) -> Vec<Pick<'_>> {
        let only_commands = self.only_commands(cx);
        let commands = self
            .matched
            .iter()
            .filter(|_| self.kind(cx).is_none() || only_commands)
            .map(|&ix| Pick::Command(&self.commands[ix]));
        let kind = self.kind(cx);
        let hits = self
            .hits
            .iter()
            .filter(|_| !only_commands)
            .filter(|hit| kind.is_none_or(|kind| hit.row.kind() == Some(kind)))
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
    fn match_commands(&mut self, cx: &App) {
        let recent = self.workspace.read(cx).recent_commands();
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

    /// What the entries are searched for, less a kind's prefix. Nothing for
    /// a query of commands.
    fn query(&self, cx: &App) -> Option<Query> {
        let content = self.field.read(cx).content();
        match content.trim_start().starts_with(COMMAND_PREFIX) {
            true => None,
            false => Query::literal(mention::Query::parse(content).rest),
        }
    }

    /// The one kind listed: the filter's, else the query's prefix's — see
    /// [`mention::Query`].
    fn kind(&self, cx: &App) -> Option<Kind> {
        match self.filter {
            Some(Filter::Kind(kind)) => Some(kind),
            Some(Filter::Commands) => None,
            None => mention::Query::parse(self.field.read(cx).content()).kind,
        }
    }

    /// The label the query's prefix names — see [`mention::Query`].
    fn label(&self, cx: &App) -> Option<String> {
        mention::Query::parse(self.field.read(cx).content()).label
    }

    fn apply(&mut self, _: &ApplySearch, _: &mut Window, cx: &mut Context<Self>) {
        if self.linking.is_some() {
            return;
        }
        let Some(query) = self.query(cx) else {
            return;
        };
        let kind = self.kind(cx);
        let _ = self.root.update(cx, |root, cx| {
            root.search
                .update(cx, |search, cx| search.apply(query, kind, cx))
        });
        cx.emit(DismissEvent);
    }

    /// List again for what the field holds. A search still running is
    /// dropped with its task.
    fn refresh(&mut self, cx: &mut Context<Self>) {
        self.match_commands(cx);
        let label = self.label(cx);
        let Some(query) = self.query(cx) else {
            self.task = None;
            self.searching = false;
            self.hits = match label {
                Some(_) => self.named_hits(cx),
                None => self
                    .recent
                    .iter()
                    .map(|row| Hit {
                        row: row.clone(),
                        snippet: None,
                        card: None,
                    })
                    .collect(),
            };
            self.limit = RECENT;
            self.selected = 0;
            cx.notify();
            return;
        };
        self.searching = true;
        let roots: Vec<PathBuf> = self
            .workspace
            .read(cx)
            .projects
            .iter()
            .map(|open| open.path.clone())
            .collect();
        self.task = Some(cx.spawn(async move |this, cx| {
            // A keystroke landing in the meantime drops this task, so a burst
            // of typing is one search.
            cx.background_executor().timer(SETTLE).await;
            let found = cx
                .background_executor()
                .spawn(async move { search_all(&roots, &query) })
                .await;
            let _ = this.update(cx, |this, cx| {
                let mut hits = this.named_hits(cx);
                let ranked: Vec<Hit> = Hit::ranked(found, this.workspace.read(cx), cx)
                    .into_iter()
                    .filter(|hit| {
                        !hits
                            .iter()
                            .any(|held| held.card.is_none() && held.row == hit.row)
                    })
                    .filter(|hit| {
                        label
                            .as_ref()
                            .is_none_or(|label| this.carries(&hit.row, label, cx))
                    })
                    .collect();
                hits.extend(ranked);
                this.hits = hits;
                this.limit = SHOWN;
                this.searching = false;
                this.selected = 0;
                cx.notify();
            });
        }));
        cx.notify();
    }

    /// Whether the entry `row` names carries `label`.
    fn carries(&self, row: &Row, label: &str, cx: &App) -> bool {
        let Row::Entry { project, showing } = row else {
            return false;
        };
        let workspace = self.workspace.read(cx);
        workspace
            .project_ix(*project)
            .and_then(|at| workspace.labels_of(at, showing, cx))
            .is_some_and(|labels| labels.iter().any(|held| held == label))
    }

    /// The entries and cards the field's query names — see
    /// [`mention::Query::rank_named`].
    fn named_hits(&self, cx: &App) -> Vec<Hit> {
        let held = cx
            .try_global::<Linkables>()
            .map_or(&[][..], |Linkables(held)| held.as_slice());
        let workspace = self.workspace.read(cx);
        let cards = mention::carded(workspace, cx);
        let (named, _) =
            mention::Query::parse(self.field.read(cx).content()).rank_named(held, &cards);
        named
            .into_iter()
            .filter_map(|named| match named {
                mention::Named::Entry(ix) => Some(Hit {
                    row: held[ix].row.clone(),
                    snippet: None,
                    card: None,
                }),
                mention::Named::Card(ix) => {
                    let carded = &cards[ix];
                    let project = workspace
                        .projects
                        .iter()
                        .find(|open| artifact::reference::names(&open.path, &carded.project))?;
                    Some(Hit {
                        row: Row::Entry {
                            project: project.id,
                            showing: Showing::Board(carded.board.id.clone()),
                        },
                        snippet: None,
                        card: Some(HitCard {
                            reference: carded.reference().to_string(),
                            handle: carded.handle.clone(),
                            title: carded.card.title(),
                        }),
                    })
                }
            })
            .collect()
    }

    /// How a session row is written as a reference. Nothing for any other
    /// row, or a session without a number yet.
    fn linked(&self, row: &Row, cx: &App) -> Option<Linked> {
        let Row::Entry {
            showing: Showing::Session(id),
            ..
        } = row
        else {
            return None;
        };
        Some(Linked {
            url: self.workspace.read(cx).link_of_session(*id, cx)?,
        })
    }

    fn select_next(&mut self, _: &SelectNext, _: &mut Window, cx: &mut Context<Self>) {
        self.step_hit(1, cx);
    }

    fn select_prev(&mut self, _: &SelectPrev, _: &mut Window, cx: &mut Context<Self>) {
        self.step_hit(-1, cx);
    }

    fn step_hit(&mut self, by: isize, cx: &mut Context<Self>) {
        let len = self.shown(cx).len();
        if len == 0 {
            return;
        }
        let at = (self.selected as isize + by).rem_euclid(len as isize) as usize;
        self.selected = at;
        // The list's children count a heading before each section.
        let commands = self
            .shown(cx)
            .iter()
            .filter(|pick| matches!(pick, Pick::Command(_)))
            .count();
        let headings = match at < commands {
            true => 1,
            false => usize::from(commands > 0) + 1,
        };
        self.scroll.scroll_to_item(at + headings);
        cx.notify();
    }

    fn open_selected(&mut self, _: &OpenHit, window: &mut Window, cx: &mut Context<Self>) {
        self.open_hit(self.selected, window, cx);
    }

    /// Open a hit, and mark the query in it where it was found in the body. A
    /// command runs on the surface the palette was raised over.
    fn open_hit(&mut self, ix: usize, window: &mut Window, cx: &mut Context<Self>) {
        let hit = match self.shown(cx).get(ix).copied() {
            Some(Pick::Hit(hit)) => hit.clone(),
            Some(Pick::Command(command)) => {
                let action = command.action.boxed_clone();
                let key = command.key();
                self.workspace
                    .update(cx, |workspace, _| workspace.ran_command(key));
                if let Some(back) = &self.returns_to {
                    window.focus(back, cx);
                }
                cx.emit(DismissEvent);
                window.dispatch_action(action, cx);
                return;
            }
            None => return,
        };
        cx.emit(DismissEvent);
        if let Some(on) = self.linking.clone() {
            let Some(linked) = self.linked(&hit.row, cx) else {
                return;
            };
            let _ = self.root.update(cx, |root, cx| {
                on(root, linked, window, cx);
                cx.notify();
            });
            return;
        }
        let query = self.field.read(cx).content().clone();
        let keep = self.keep;
        let _ = self.root.update(cx, |root, cx| {
            if let Some(card) = &hit.card {
                root.open_reference(&card.reference, window, cx);
                cx.notify();
                return;
            }
            let opening = match keep {
                true => Opening::Kept,
                false => Opening::Preview,
            };
            root.open_row(&hit.row, opening, window, cx);
            if hit.snippet.is_some()
                && let Some(pane) = root.leaf().cloned()
            {
                pane.update(cx, |pane, cx| {
                    pane.finding = true;
                    pane.find_field
                        .update(cx, |field, cx| field.set_content(query, cx));
                    pane.push_find(cx);
                });
            }
            cx.notify();
        });
    }

    /// While linking, the last turns of the session the selection is on.
    fn link_preview(&self, window: &mut Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        self.linking.as_ref()?;
        let Some(Pick::Hit(Hit {
            row:
                Row::Entry {
                    showing: Showing::Session(id),
                    ..
                },
            ..
        })) = self.shown(cx).get(self.selected).copied()
        else {
            return None;
        };
        let id = *id;
        let theme = Theme::of(cx).clone();
        let count = self.workspace.update(cx, |workspace, cx| {
            workspace.load_history(id, cx);
            let chat = workspace.session(id, cx)?;
            Some(artifact::session::chat::turns(&chat.items).len())
        });
        let body = count.map(|count| {
            let from = count.saturating_sub(crate::view::kit::link::PREVIEW_TURNS);
            let (workspace, root) = (self.workspace.clone(), self.root.clone());
            window
                .use_keyed_state(
                    SharedString::from(format!("search-preview-{id}-{from}-{count}")),
                    cx,
                    move |_, cx| {
                        crate::view::component::transcript::SessionView::new(
                            id,
                            None,
                            crate::view::component::transcript::Drawn::Excerpt { from, to: count },
                            crate::view::kit::link::CARD_WIDTH,
                            workspace,
                            root,
                            cx,
                        )
                    },
                )
                .into_any_element()
        });
        Some(
            div()
                .id("search-preview")
                .w(px(280.))
                .flex_none()
                .border_l_1()
                .border_color(theme.border)
                .map(|el| bezel::ui::scroll::scrolls(el, bezel::ui::scroll::Axes::Vertical))
                .children(body)
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
        let filters = self.filters();
        let at = filters
            .iter()
            .position(|(kind, _)| *kind == self.filter)
            .unwrap_or(0) as isize;
        let at = (at + by).rem_euclid(filters.len() as isize) as usize;
        self.set_filter(filters[at].0, cx);
    }

    /// The kinds on offer: every one, or sessions alone while linking.
    fn filters(&self) -> Vec<(Option<Filter>, &'static str)> {
        FILTERS
            .into_iter()
            .filter(|(kind, _)| {
                self.linking.is_none() || *kind == Some(Filter::Kind(Kind::Session))
            })
            .collect()
    }

    fn set_filter(&mut self, filter: Option<Filter>, cx: &mut Context<Self>) {
        self.filter = filter;
        self.match_commands(cx);
        self.selected = 0;
        self.scroll.scroll_to_item(0);
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
                    .child(hint(
                        "↵",
                        match self.linking {
                            Some(_) => "Link",
                            None => "Open",
                        },
                    )),
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
            .children(
                self.filters()
                    .into_iter()
                    .enumerate()
                    .map(|(ix, (kind, label))| {
                        let on = kind == self.filter;
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
                            .child(label)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                cx.stop_propagation();
                                this.set_filter(kind, cx);
                            }))
                    }),
            )
            .into_any_element()
    }

    fn command_row(
        &self,
        ix: usize,
        command: &menubar::Command,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = Theme::of(cx).clone();
        let selected = ix == self.selected;
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
        let project = hit
            .row
            .located(self.workspace.read(cx))
            .and_then(|(project, _)| {
                self.workspace
                    .read(cx)
                    .projects
                    .get(project)
                    .map(|open| open.name())
            });
        let icon = kind_icon(hit.row.kind().unwrap_or(Kind::Article));
        let title = match &hit.card {
            Some(card) => format!("{} · {}", card.handle, card.title),
            None => hit.row.label(self.workspace.read(cx), cx),
        };
        let selected = ix == self.selected;
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

impl EventEmitter<DismissEvent> for Palette {}

impl Focusable for Palette {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.field.focus_handle(cx)
    }
}

impl Render for Palette {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = Theme::of(cx).clone();
        let empty = self.query(cx).is_none();
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
        for (ix, pick) in self.shown(cx).into_iter().enumerate() {
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
        let preview = self.link_preview(window, cx);
        let note = match (rows.is_empty(), self.searching, empty) {
            (false, ..) => None,
            (true, true, _) => Some("Searching…"),
            (true, false, true) => Some("Nothing yet"),
            (true, false, false) => Some("No matches"),
        };
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
            .child(bezel::ui::cover::cover())
            .on_click(cx.listener(|_, _, _, cx| cx.emit(DismissEvent)))
            .child(
                div()
                    .id("search-palette")
                    .w(px(if preview.is_some() { 720. } else { 560. }))
                    .max_h(px(440.))
                    .flex()
                    .flex_col()
                    .rounded(px(Theme::panel_radius()))
                    .border_1()
                    .border_color(theme.border)
                    .bg(theme.surface)
                    .overflow_hidden()
                    .on_click(|_, _, cx| cx.stop_propagation())
                    .on_action(cx.listener(Self::select_next))
                    .on_action(cx.listener(Self::select_prev))
                    .on_action(cx.listener(Self::open_selected))
                    .on_action(cx.listener(Self::apply))
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
                            .child(div().flex_1().min_w_0().child(self.field.clone())),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_h_0()
                            .flex()
                            .flex_row()
                            .child(
                                div()
                                    .id("search-hits")
                                    .flex_1()
                                    .min_h_0()
                                    .map(|el| {
                                        bezel::ui::scroll::scrolls(
                                            el,
                                            bezel::ui::scroll::Axes::Vertical,
                                        )
                                    })
                                    .track_scroll(&self.scroll)
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
                                    }))
                                    .min_w_0(),
                            )
                            .children(preview),
                    )
                    .child(self.palette_footer(cx))
                    .when(self.linking.is_none(), |palette| {
                        palette.child(
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
                                    this.apply(&ApplySearch, window, cx)
                                })),
                        )
                    }),
            )
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

#[cfg(test)]
#[path = "../../../tests/unit/search_palette.rs"]
mod tests;
