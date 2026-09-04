//! The projects sidebar: a folding heading per project, and every session,
//! article and table in it. The window's grid lives in [`crate::view::root`];
//! this draws on it.

use crate::{
    model::session::ChatSession,
    view::{
        component::menu::{self, Menu},
        root::{self, CommitName, Cydonia, DismissName, NewSession, OpenProject, Pane},
        settings::Section,
    },
};
use bezel::{
    gpui::{
        self, AnyElement, App, Context, Div, Empty, Focusable as _, FontWeight, Hsla, MouseButton,
        ScrollStrategy, SharedString, Stateful, Window, div, prelude::*, px, svg, uniform_list,
    },
    motion::Painter,
    theme::{TextStyle, Theme, Typeset},
    ui::{
        icons, loaders,
        menu::Item,
        popover,
        surface::Surfaced as _,
        tooltip::Tooltip,
        widgets::{Buttons, Layout},
    },
};
use std::{cmp::Reverse, ops::Range, path::PathBuf};

/// What the sidebar needs of a session to draw its row, read out of the model
/// before the row is built: a turn in flight puts a thinking orb in the mark's
/// place, and the orb leases the frame clock, which wants the app mutably.
struct SessionRow {
    project: usize,
    id: u64,
    label: String,
    icon: Option<SharedString>,
    streaming: bool,
    archived: bool,
}

/// One line of the sidebar. An address, not content: the label behind it is
/// read when the row is built, which [`uniform_list`] only does for the rows on
/// screen.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Row {
    Project(usize),
    /// The line the archived entries are folded under.
    Archive(usize),
    Session {
        project: usize,
        id: u64,
    },
    Board {
        project: usize,
        ix: usize,
    },
    Article {
        project: usize,
        ix: usize,
    },
    Table {
        project: usize,
        ix: usize,
    },
}

/// Which kinds the sidebar lists. One choice for the whole column, above the
/// projects, because it answers "what am I looking for", not "what is in this
/// project".
#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum Filter {
    #[default]
    All,
    Sessions,
    Boards,
    Articles,
    Tables,
}

impl Filter {
    /// Sessions are only on offer while agents are: a kind you cannot make is
    /// not a kind worth filtering to.
    fn every(agents: bool) -> Vec<Self> {
        let mut every = vec![Self::All];
        if agents {
            every.push(Self::Sessions);
        }
        every.extend([Self::Boards, Self::Articles, Self::Tables]);
        every
    }

    fn label(self) -> &'static str {
        match self {
            Self::All => "All",
            Self::Sessions => "Sessions",
            Self::Boards => "Boards",
            Self::Articles => "Articles",
            Self::Tables => "Tables",
        }
    }

    fn icon(self) -> &'static str {
        match self {
            Self::All => icons::SORT_VERTICAL,
            Self::Sessions => icons::CHAT_ROUND_LINE,
            Self::Boards => icons::LIST,
            Self::Articles => icons::DOCUMENT,
            Self::Tables => icons::WIDGET,
        }
    }

    /// Whether an entry is one of the kind being looked for.
    fn keeps(self, row: Row) -> bool {
        matches!(
            (self, row),
            (Self::All, _)
                | (Self::Sessions, Row::Session { .. })
                | (Self::Boards, Row::Board { .. })
                | (Self::Articles, Row::Article { .. })
                | (Self::Tables, Row::Table { .. })
        )
    }
}

/// What the sidebar's name field is attached to. One field for all of them,
/// because only one row can be being named at a time. Each entry is held by
/// what identifies it — a file, a session, a table's key — never by an index:
/// that moves the moment a neighbour is made or dropped, and the field would
/// follow it onto whichever entry slid underneath.
#[derive(Clone, PartialEq, Eq)]
pub(crate) enum Renaming {
    Session(u64),
    Board(PathBuf),
    Article(PathBuf),
    Table(String),
}

/// What an entry's row is written in: the one on screen at full strength, one
/// put away a step back from the rest.
pub(crate) fn tint(selected: bool, archived: bool, theme: &Theme) -> Hsla {
    match (selected, archived) {
        (true, _) => theme.text,
        (false, true) => theme.text_faint,
        (false, false) => theme.text_muted,
    }
}

/// A project on its way to another place in the list. The index is safe to
/// carry: nothing reorders the list while a drag is in flight.
#[derive(Clone)]
pub(crate) struct ProjectDrag(usize);

/// What rides under the cursor while a project is being carried.
struct Carried(SharedString);

impl Render for Carried {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = Theme::of(cx).clone();
        popover::popover_card(&theme)
            .px(px(10.))
            .py(px(4.))
            .text_style(TextStyle::Callout)
            .text_color(theme.text)
            .child(self.0.clone())
    }
}

/// An entry's own name in the element tree: two rows must never share one.
fn key_of(entry: Row) -> String {
    match entry {
        Row::Project(ix) => format!("project-{ix}"),
        Row::Archive(ix) => format!("archive-{ix}"),
        Row::Session { project, id } => format!("session-{project}-{id}"),
        Row::Board { project, ix } => format!("board-{project}-{ix}"),
        Row::Article { project, ix } => format!("article-{project}-{ix}"),
        Row::Table { project, ix } => format!("table-{project}-{ix}"),
    }
}

/// The wash a row paints, and — with the 1px either side of it that used to be
/// the column's gap — the pitch the list lays every row out at. One height for
/// headings and rows alike, because [`uniform_list`] measures a single row and
/// gives every other one the same.
const ROW_PILL: f32 = 30.;
pub(crate) const ROW_HEIGHT: f32 = ROW_PILL + 2.;

/// The box every row under a project heading sits in: indented beneath the
/// heading, and carrying the wash that says which one is open.
///
/// Shared because the indent is a measurement three files have to agree on.
/// Written out in each of them, it drifts.
pub(crate) fn row(
    id: impl Into<gpui::ElementId>,
    group: &'static str,
    selected: bool,
    theme: &Theme,
) -> Stateful<Div> {
    div()
        .id(id)
        .group(group)
        .h(px(ROW_PILL))
        .ml(px(root::SIDEBAR_GUTTER))
        .mr(px(root::SIDEBAR_GUTTER))
        .px(px(root::SIDEBAR_GUTTER))
        .flex()
        .flex_row()
        .items_center()
        .gap(px(8.))
        .rounded(px(Theme::control_radius()))
        .cursor_pointer()
        .when(selected, |el| el.bg(theme.element_active))
        // Only off the open row: the hover wash is the weaker rung, and
        // painting it over the selection would dim what the pointer is on.
        .when(!selected, |el| el.hover(|el| el.bg(theme.element_hover)))
}

/// The inset [`Buttons::control_group`] holds its controls at, mirrored here
/// because the height below is measured from it and bezel keeps it private.
const CLUSTER_PAD: f32 = 2.;

/// The floating cluster's height, half of which is the pill's radius: a ghost
/// button's box — a 14pt glyph in 4pt of padding — inside that inset.
const CLUSTER_HEIGHT: f32 = 14. + 2. * 4. + 2. * CLUSTER_PAD;

/// A row's name. The line height is what the field pins itself to: left to
/// gpui's default the label's box is φ×13, and renaming would resize the row
/// under the name being typed.
fn row_label(name: String, tint: Hsla) -> AnyElement {
    div()
        .flex_1()
        .min_w_0()
        .truncate()
        .text_style(TextStyle::Body)
        .line_height(px(18.))
        .text_color(tint)
        .child(name)
        .into_any_element()
}

impl Cydonia {
    pub(crate) fn sidebar(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let theme = Theme::of(cx).clone();
        let rows = self.rows(cx);
        let count = rows.len();
        div()
            .flex_none()
            .w(px(self.sidebar_width))
            .h_full()
            .bg(root::sidebar_bg(&theme))
            // Drawn ON the column, not left as a gap between two: a bare strip
            // between them would be raw desktop at full strength, a bright line
            // the height of the window.
            .border_r_1()
            .border_color(theme.border)
            .flex()
            .flex_col()
            // The fold out at the trailing edge: the lights float in the
            // leading half of the strip, which is what leaves nothing there to
            // pad them clear of.
            .child(
                div()
                    .flex_none()
                    .h(px(root::HEADER_HEIGHT))
                    .pr(px(8.))
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_end()
                    .child(self.fold_toggle(theme.text_faint, cx)),
            )
            .child(
                uniform_list(
                    "project-list",
                    count,
                    cx.processor(move |this, range: Range<usize>, _, cx| {
                        range.map(|ix| this.sidebar_row(rows[ix], cx)).collect()
                    }),
                )
                .track_scroll(&self.rail)
                .flex_1()
                .min_h_0(),
            )
            .child(
                div()
                    .flex_none()
                    .mx(px(8.))
                    .mb(px(8.))
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_between()
                    .child(
                        theme
                            .ghost("settings")
                            .px(px(8.))
                            .py(px(6.))
                            .gap(px(8.))
                            .child(
                                icons::icon(icons::SETTINGS_MINIMALISTIC)
                                    .size(px(13.))
                                    .text_color(theme.text_faint),
                            )
                            .child(
                                div()
                                    .text_style(TextStyle::Body)
                                    .text_color(theme.text_muted)
                                    .child("Settings"),
                            )
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.open_settings(Section::Appearance, cx)
                            })),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap(px(2.))
                            .child(self.filter_button(cx))
                            .child(
                                theme
                                    .ghost("open-project")
                                    .px(px(8.))
                                    .py(px(6.))
                                    .tooltip(|window, cx| {
                                        Tooltip::with_keystroke("New project", "⌘O", window, cx)
                                    })
                                    .child(
                                        icons::icon(icons::DOCUMENT_ADD)
                                            .size(px(13.))
                                            .text_color(theme.text_faint),
                                    )
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.open_project_action(&OpenProject, window, cx);
                                    })),
                            ),
                    ),
            )
    }

    /// The fold toggle once the sidebar is away, as a glass pill over the
    /// content. Out of flow and hugging the one control it holds: a band would
    /// take a row off every pane to carry a single button, and the column under
    /// it is what the button is for. Only the fold — adding a project acts on
    /// the list you are looking at, and with the list gone it is chrome for
    /// somewhere you are not. Its tone is the strong one, because the plate
    /// floats over whatever the pane shows, which can be a picture we did not
    /// choose.
    pub(crate) fn fold_cluster(&self, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let theme = Theme::of(cx).clone();
        theme
            .control_group()
            .absolute()
            .top(px((root::HEADER_HEIGHT - CLUSTER_HEIGHT) / 2.))
            // Full screen takes the lights away, and the room they needed
            // would be left as a hole.
            .left(px(if window.is_fullscreen() {
                root::HEADER_INSET
            } else {
                root::TOOLBAR_INSET
            }))
            .h(px(CLUSTER_HEIGHT))
            // A pill, where the group's own corner is cut for a row of square
            // buttons. Before the glass, which reads the corners off the box.
            .rounded(px(CLUSTER_HEIGHT / 2.))
            .items_center()
            .child(self.fold_toggle(theme.text, cx))
            // The same glass bezel's own floating bar mounts on. Its
            // `control_bar` is the shipped container, and it refuses this case
            // on purpose: a fixed 56pt tall, and sized by its caller rather
            // than by what it holds.
            .surface(&theme, theme.popover_surface)
            .into_any_element()
    }

    /// The control that folds the sidebar away and brings it back. It belongs
    /// to whichever column runs along the window's left edge, so it changes
    /// strip across the collapse — and takes that strip's tone with it.
    fn fold_toggle(&self, tint: Hsla, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let theme = Theme::of(cx).clone();
        let label = if self.sidebar_open {
            "Hide sidebar"
        } else {
            "Show sidebar"
        };
        theme
            .ghost("toggle-sidebar")
            .p(px(4.))
            .tooltip(move |window, cx| Tooltip::text(label, window, cx))
            .child(
                icons::icon(icons::SIDEBAR_MINIMALISTIC_LEFT)
                    .size(px(14.))
                    .text_color(tint),
            )
            .on_click(cx.listener(|this, _, _, cx| this.toggle_sidebar(cx)))
    }

    /// One project's heading: it folds, and its `+` opens what can be made in
    /// the project.
    fn project_head(&self, ix: usize, cx: &mut Context<Self>) -> AnyElement {
        let theme = Theme::of(cx).clone();
        let (name, expanded) = match self.workspace.read(cx).projects.get(ix) {
            Some(project) => (project.name(), project.expanded),
            None => return Empty.into_any_element(),
        };
        let carried = SharedString::from(name.clone());
        let head = div()
            .id(("project", ix))
            .group("project-head")
            .mx(px(8.))
            .px(px(6.))
            .h(px(ROW_PILL))
            .rounded(px(Theme::control_radius()))
            .relative()
            .flex()
            .flex_row()
            .items_center()
            .gap(px(4.))
            .cursor_pointer()
            // On the head, not the label: a name's colour is fixed when
            // its text is laid out, and only this div is stateful enough
            // to carry the hover that far.
            .text_color(theme.text_faint)
            .hover(|el| el.text_color(theme.text))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_style(TextStyle::Callout)
                    .font_weight(FontWeight::MEDIUM)
                    .child(name),
            )
            .child(
                self.menu_button(
                    ("project-add", ix),
                    "project-head",
                    icons::icon(icons::PLUS)
                        .size(px(12.))
                        .text_color(theme.text_faint)
                        .group_hover("project-head", |el| el.text_color(theme.text)),
                    Menu::Add(ix),
                    cx,
                )
                .children(self.add_menu(ix, cx)),
            )
            .child(
                theme
                    .ghost(("project-fold", ix))
                    .flex_none()
                    .p(px(3.))
                    .invisible()
                    .group_hover("project-head", |el| el.visible())
                    .child(
                        theme
                            .disclosure(expanded)
                            .text_color(theme.text_faint)
                            .group_hover("project-head", |el| el.text_color(theme.text)),
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        cx.stop_propagation();
                        this.toggle_project(ix, cx);
                    })),
            )
            .children(self.project_menu(ix, cx))
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |this, _, _, cx| this.toggle_menu(Menu::Project(ix), cx)),
            )
            .on_click(cx.listener(move |this, _, _, cx| this.toggle_project(ix, cx)))
            // Carried by its heading, and dropped on the heading it is to sit
            // in front of. Nothing else in the column is draggable: what the
            // entries are ordered by is when they were last written.
            .on_drag(ProjectDrag(ix), move |_, _, _, cx| {
                let carried = carried.clone();
                cx.new(|_| Carried(carried))
            })
            .drag_over::<ProjectDrag>(move |style, _, _, cx| style.bg(Theme::of(cx).element_active))
            .on_drop(cx.listener(move |this, drag: &ProjectDrag, _, cx| {
                this.move_project(drag.0, ix, cx);
            }));
        // Its own menu opens on the press rather than the click, so the note
        // has to be here too — read stale, a right press would swallow.
        self.menu_press(head, Menu::Project(ix), cx)
            .into_any_element()
    }

    /// Everything open in one project, last written first — the lines the
    /// sidebar draws under its head, and the ring a keyboard step walks.
    ///
    /// One list rather than four: the kinds are told apart by their marks, and
    /// grouping by kind buries the table you are working in under every article
    /// you are not. Only the addresses are ordered — each kind's own list keeps
    /// the indices these carry.
    pub(crate) fn entries(&self, project: usize, cx: &App) -> Vec<Row> {
        let Some(open) = self.workspace.read(cx).projects.get(project) else {
            return Vec::new();
        };
        let sessions = open.sessions.iter().map(|chat| {
            (
                chat.closed,
                chat.touched(),
                Row::Session {
                    project,
                    id: chat.id,
                },
            )
        });
        let boards = open
            .boards
            .iter()
            .enumerate()
            .map(|(ix, board)| (board.archived, board.touched, Row::Board { project, ix }));
        let articles = open.articles.iter().enumerate().map(|(ix, article)| {
            (
                article.archived,
                article.touched,
                Row::Article { project, ix },
            )
        });
        // The store keeps seconds; every other stamp here is milliseconds.
        let tables = open.tables.iter().enumerate().map(|(ix, table)| {
            let at = table.updated_at.unwrap_or(table.created_at).max(0) as u128;
            (table.archived, at * 1000, Row::Table { project, ix })
        });
        let mut entries: Vec<(bool, u128, Row)> = sessions
            .chain(boards)
            .chain(articles)
            .chain(tables)
            .collect();
        entries.retain(|(_, _, row)| self.filter.keeps(*row));
        // One sort for both halves: what was put away sinks, and inside each
        // half the last thing written is on top.
        entries.sort_by_key(|(archived, touched, _)| (*archived, Reverse(*touched)));
        let split = entries.iter().position(|(archived, ..)| *archived);
        let mut rows: Vec<Row> = entries
            .iter()
            .take(split.unwrap_or(entries.len()))
            .map(|(_, _, row)| *row)
            .collect();
        if let Some(split) = split {
            rows.push(Row::Archive(project));
            if open.archive_open {
                rows.extend(entries[split..].iter().map(|(_, _, row)| *row));
            }
        }
        rows
    }

    /// Every line the sidebar shows, in order. Addresses only: a project with a
    /// thousand articles costs a thousand `Row`s here and reads a title for
    /// none of them.
    pub(crate) fn rows(&self, cx: &Context<Self>) -> Vec<Row> {
        let mut rows = Vec::new();
        for p in 0..self.workspace.read(cx).projects.len() {
            rows.push(Row::Project(p));
            if self.workspace.read(cx).projects[p].expanded {
                rows.extend(self.entries(p, cx));
            }
        }
        rows
    }

    /// Open what a row points at — what a keyboard step does with its landing.
    /// The pointer never comes through here: each row carries its own
    /// `on_click`, which needs no [`Row`] to know what it is.
    pub(crate) fn open_row(&mut self, row: Row, window: &mut Window, cx: &mut Context<Self>) {
        match row {
            Row::Project(ix) => self.select_project(ix, cx),
            Row::Archive(ix) => self.toggle_archive(ix, cx),
            Row::Session { id, .. } => self.select_session(id, cx),
            Row::Board { project, ix } => self.open_board(project, ix, cx),
            Row::Article { project, ix } => self.open_article(project, ix, window, cx),
            Row::Table { project, ix } => self.open_table(project, ix, cx),
        }
    }

    /// Scroll the rail to a row, if it is not already on screen. `Nearest`
    /// rather than `Top`: a step to the neighbour below should move the list by
    /// a row, not throw the one you came from off the top of it.
    pub(crate) fn reveal(&mut self, row: Row, cx: &Context<Self>) {
        if let Some(ix) = self.rows(cx).iter().position(|at| *at == row) {
            self.rail.scroll_to_item(ix, ScrollStrategy::Nearest);
        }
    }

    /// One line, built when the list scrolls it into view. The box around it is
    /// what holds the pitch: the row inside paints the wash, and the pixel
    /// either side of it is the gap between two.
    fn sidebar_row(&self, row: Row, cx: &mut Context<Self>) -> AnyElement {
        let workspace = self.workspace.read(cx);
        let inner = match row {
            Row::Project(ix) => self.project_head(ix, cx),
            Row::Archive(ix) => self.archive_divider(ix, cx),
            Row::Session { project, id } => match self.session_of(project, id, cx) {
                Some(session) => self.session_row(session, cx),
                None => Empty.into_any_element(),
            },
            Row::Board { project, ix } => {
                match workspace
                    .projects
                    .get(project)
                    .and_then(|open| open.boards.get(ix).map(|board| board.label().to_owned()))
                {
                    Some(name) => self.board_row(project, ix, name, cx),
                    None => Empty.into_any_element(),
                }
            }
            Row::Article { project, ix } => {
                match workspace.projects.get(project).and_then(|open| {
                    open.articles
                        .get(ix)
                        .map(|article| article.label().to_owned())
                }) {
                    Some(title) => self.article_row(project, ix, title, cx).into_any_element(),
                    None => Empty.into_any_element(),
                }
            }
            Row::Table { project, ix } => {
                match workspace
                    .projects
                    .get(project)
                    .and_then(|open| open.tables.get(ix).map(|table| table.name.clone()))
                {
                    Some(name) => self.table_row(project, ix, name, cx).into_any_element(),
                    None => Empty.into_any_element(),
                }
            }
        };
        div()
            .h(px(ROW_HEIGHT))
            .py(px(1.))
            .child(inner)
            .into_any_element()
    }

    /// What the sidebar needs of a session, read when its row comes on screen.
    fn session_of(&self, project: usize, id: u64, cx: &Context<Self>) -> Option<SessionRow> {
        let workspace = self.workspace.read(cx);
        let chat = workspace.projects.get(project)?.session(id)?;
        Some(SessionRow {
            project,
            id: chat.id,
            label: chat.label(),
            icon: workspace.agent_icon(&chat.entry.name),
            streaming: chat.streaming,
            archived: chat.closed,
        })
    }

    /// The line the archive folds under: what is put away is still listed, a
    /// step below everything still in hand.
    fn archive_divider(&self, project: usize, cx: &mut Context<Self>) -> AnyElement {
        let theme = Theme::of(cx).clone();
        let open = self
            .workspace
            .read(cx)
            .projects
            .get(project)
            .is_some_and(|open| open.archive_open);
        row(("archive", project), "archive-row", false, &theme)
            .child(theme.disclosure(open).text_color(theme.text_faint))
            .child(
                div()
                    .flex_none()
                    .text_style(TextStyle::Callout)
                    .text_color(theme.text_faint)
                    .child("Archived"),
            )
            .child(div().flex_1().h(px(1.)).bg(theme.border))
            .on_click(cx.listener(move |this, _, _, cx| this.toggle_archive(project, cx)))
            .into_any_element()
    }

    fn toggle_archive(&mut self, project: usize, cx: &mut Context<Self>) {
        self.commit(cx);
        self.workspace.update(cx, |workspace, cx| {
            if let Some(open) = workspace.projects.get_mut(project) {
                open.archive_open = !open.archive_open;
            }
            cx.notify();
        });
    }

    /// Menus address a project by its place in the list, so the one open when
    /// it moves would be pointing at whichever project slid underneath.
    fn move_project(&mut self, from: usize, to: usize, cx: &mut Context<Self>) {
        self.menu = None;
        self.workspace
            .update(cx, |workspace, cx| workspace.move_project(from, to, cx));
    }

    fn toggle_project(&mut self, ix: usize, cx: &mut Context<Self>) {
        self.commit(cx);
        self.workspace.update(cx, |workspace, cx| {
            if let Some(project) = workspace.projects.get_mut(ix) {
                project.expanded = !project.expanded;
            }
            cx.notify();
        });
    }

    /// The kind picker: a button in the footer, carrying the mark of whatever
    /// it is narrowed to, so the column says what it is showing without a line
    /// of its own to say it in.
    fn filter_button(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = Theme::of(cx).clone();
        let label = self.filter.label();
        let trigger = theme
            .ghost("filter")
            .relative()
            .px(px(8.))
            .py(px(6.))
            .tooltip(move |window, cx| Tooltip::text(format!("Showing {label}"), window, cx))
            .child(
                icons::icon(self.filter.icon())
                    .size(px(13.))
                    .text_color(theme.text_faint),
            )
            .on_click(cx.listener(|this, _, _, cx| {
                cx.stop_propagation();
                this.toggle_menu(Menu::Filter, cx);
            }))
            .children(self.filter_menu(cx));
        self.menu_press(trigger, Menu::Filter, cx)
            .into_any_element()
    }

    fn filter_menu(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if self.menu != Some(Menu::Filter) {
            return None;
        }
        let agents = self.workspace.read(cx).settings.agents_enabled;
        let rows = Filter::every(agents)
            .into_iter()
            .map(|filter| {
                menu::row(
                    Item::action(filter.label())
                        .with_icon(filter.icon())
                        .checked(self.filter == filter),
                    move |this, _, cx| {
                        this.filter = filter;
                        this.menu = None;
                        cx.notify();
                    },
                )
            })
            .collect();
        Some(popover::anchored_menu_above(
            "filter-menu",
            self.menu_card("filter-menu", rows, cx),
            None,
        ))
    }

    /// What the `+` starts here. Session first: it is what the sidebar is for.
    fn add_menu(&self, ix: usize, cx: &mut Context<Self>) -> Option<AnyElement> {
        if self.menu != Some(Menu::Add(ix)) {
            return None;
        }
        let mut rows = Vec::new();
        if self.workspace.read(cx).settings.agents_enabled {
            rows.push(menu::row(
                Item::action("New session").with_icon(icons::CHAT_ROUND_LINE),
                move |this, window, cx| {
                    this.select_project(ix, cx);
                    this.new_session_action(&NewSession, window, cx);
                },
            ));
        }
        rows.extend([
            menu::row(
                Item::action("New board").with_icon(icons::LIST),
                move |this, _, cx| this.new_board(ix, cx),
            ),
            menu::row(
                Item::action("New article").with_icon(icons::DOCUMENT_ADD),
                move |this, window, cx| this.new_article(ix, window, cx),
            ),
            menu::row(
                Item::action("New table").with_icon(icons::WIDGET),
                move |this, _, cx| this.new_table(ix, cx),
            ),
        ]);
        let id = SharedString::from(format!("add-menu-{ix}"));
        Some(popover::anchored_menu_below(
            id.clone(),
            self.menu_card(id, rows, cx),
            None,
        ))
    }

    /// What a press on the heading opens. Removing closes the tab — the
    /// directory and everything in it stays where it is.
    fn project_menu(&self, ix: usize, cx: &mut Context<Self>) -> Option<AnyElement> {
        if self.menu != Some(Menu::Project(ix)) {
            return None;
        }
        let rows = vec![menu::row(
            Item::action("Remove project").with_icon(icons::TRASH_BIN_MINIMALISTIC),
            move |this, _, cx| this.close_project(ix, cx),
        )];
        let id = SharedString::from(format!("project-menu-{ix}"));
        Some(popover::anchored_menu_below(
            id.clone(),
            self.menu_card(id, rows, cx),
            None,
        ))
    }

    /// One session: its mark and its name.
    fn session_row(&self, session: SessionRow, cx: &mut Context<Self>) -> AnyElement {
        let entry = Row::Session {
            project: session.project,
            id: session.id,
        };
        let theme = Theme::of(cx).clone();
        let painter = Painter::of(cx);
        let id = session.id;
        let selected =
            self.showing(cx) == Pane::Chat && self.workspace.read(cx).active_id() == Some(id);
        let tint = tint(selected, session.archived, &theme);
        // The agent's own mark, in the label's colour rather than any of its
        // own: every icon the registry publishes is a `currentColor` glyph, so
        // tinting is the only colour it will ever have. While a turn is in
        // flight the orb stands in its place — the same one the transcript
        // works under.
        let mark = if session.streaming {
            loaders::orb(
                loaders::Orb::Cluster,
                SharedString::from(format!("session-orb-{id}")),
                14.,
                &theme,
                painter,
                cx,
            )
            .into_any_element()
        } else {
            match session.icon {
                Some(path) => svg()
                    .path(path)
                    .size(px(14.))
                    .flex_none()
                    .text_color(tint)
                    .into_any_element(),
                None => Empty.into_any_element(),
            }
        };

        let label = match self.renaming == Some(Renaming::Session(id)) {
            true => self.name_field(cx),
            false => row_label(session.label, tint),
        };

        row(("session", id), "session-row", selected, &theme)
            .child(
                div()
                    .flex_none()
                    .size(px(14.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(mark),
            )
            .child(label)
            .child(
                self.menu_button(
                    ("session-menu", id),
                    "session-row",
                    icons::icon(icons::MENU_DOTS)
                        .size(px(14.))
                        .text_color(theme.text_faint),
                    Menu::Entry(entry),
                    cx,
                )
                .children(self.entry_menu(entry, session.archived, cx)),
            )
            .on_click(cx.listener(move |this, _, _, cx| {
                this.select_session(id, cx);
            }))
            .into_any_element()
    }

    /// One board: its mark and its name.
    fn board_row(
        &self,
        project: usize,
        ix: usize,
        name: String,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = Theme::of(cx).clone();
        let workspace = self.workspace.read(cx);
        let selected = self.showing(cx) == Pane::Board
            && workspace.active == Some(project)
            && workspace
                .projects
                .get(project)
                .is_some_and(|open| open.board == Some(ix));
        let entry = Row::Board { project, ix };
        let board = workspace
            .projects
            .get(project)
            .and_then(|open| open.boards.get(ix));
        let archived = board.is_some_and(|board| board.archived);
        let path = board.map(|board| &board.path);
        let renaming = matches!(&self.renaming, Some(Renaming::Board(at)) if Some(at) == path);
        let tint = tint(selected, archived, &theme);
        let label = match renaming {
            true => self.name_field(cx),
            false => row_label(name, tint),
        };

        row(
            SharedString::from(format!("board-{project}-{ix}")),
            "board-row",
            selected,
            &theme,
        )
        .child(
            icons::icon(icons::LIST)
                .size(px(14.))
                .flex_none()
                .text_color(tint),
        )
        .child(label)
        .child(
            self.menu_button(
                SharedString::from(format!("board-menu-{project}-{ix}")),
                "board-row",
                icons::icon(icons::MENU_DOTS)
                    .size(px(14.))
                    .text_color(theme.text_faint),
                Menu::Entry(entry),
                cx,
            )
            .children(self.entry_menu(entry, archived, cx)),
        )
        .on_click(cx.listener(move |this, _, _, cx| this.open_board(project, ix, cx)))
        .into_any_element()
    }

    /// The `···` on any entry: the same two things whichever kind it is, and
    /// no third — nothing here deletes.
    pub(crate) fn entry_menu(
        &self,
        entry: Row,
        archived: bool,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if self.menu != Some(Menu::Entry(entry)) {
            return None;
        }
        let put = match archived {
            true => Item::action("Unarchive").with_icon(icons::ARCHIVE_MINIMALISTIC),
            false => Item::action("Archive").with_icon(icons::ARCHIVE_MINIMALISTIC),
        };
        let rows = vec![
            menu::row(
                Item::action("Rename").with_icon(icons::PEN_NEW_SQUARE),
                move |this, window, cx| this.rename_entry(entry, window, cx),
            ),
            menu::row(put, move |this, _, cx| {
                this.archive_entry(entry, !archived, cx)
            }),
        ];
        let id = SharedString::from(format!("entry-menu-{}", key_of(entry)));
        Some(popover::anchored_menu_below(
            id.clone(),
            self.menu_card(id, rows, cx),
            None,
        ))
    }

    /// Put the name field on an entry's row, whichever kind it is. Each is
    /// addressed by what identifies it, so the field cannot slide onto its
    /// neighbour if the list reorders under it.
    fn rename_entry(&mut self, entry: Row, window: &mut Window, cx: &mut Context<Self>) {
        let workspace = self.workspace.read(cx);
        let what = match entry {
            Row::Session { id, .. } => Some(Renaming::Session(id)),
            Row::Board { project, ix } => workspace
                .projects
                .get(project)
                .and_then(|open| open.boards.get(ix))
                .map(|board| Renaming::Board(board.path.clone())),
            Row::Article { project, ix } => workspace
                .projects
                .get(project)
                .and_then(|open| open.articles.get(ix))
                .map(|article| Renaming::Article(article.path.clone())),
            Row::Table { project, ix } => workspace
                .projects
                .get(project)
                .and_then(|open| open.tables.get(ix))
                .map(|table| Renaming::Table(table.key.clone())),
            Row::Project(_) | Row::Archive(_) => None,
        };
        if let Some(what) = what {
            self.start_rename(what, window, cx);
        }
    }

    /// Put an entry away, or bring it back. Where the flag lives is each
    /// kind's own business — a board's file, an article's properties, a row in
    /// the store — and the sidebar asks for it the same way.
    fn archive_entry(&mut self, entry: Row, archived: bool, cx: &mut Context<Self>) {
        self.workspace.update(cx, |workspace, cx| match entry {
            Row::Session { id, .. } => workspace.archive_session(id, archived, cx),
            Row::Board { project, ix } => {
                if let Some(path) = workspace
                    .projects
                    .get(project)
                    .and_then(|open| open.boards.get(ix))
                    .map(|board| board.path.clone())
                {
                    workspace.archive_board(&path, archived, cx);
                }
            }
            Row::Article { project, ix } => {
                if let Some(path) = workspace
                    .projects
                    .get(project)
                    .and_then(|open| open.articles.get(ix))
                    .map(|article| article.path.clone())
                {
                    workspace.archive_article(&path, archived, cx);
                }
            }
            Row::Table { project, ix } => {
                if let Some(key) = workspace
                    .projects
                    .get(project)
                    .and_then(|open| open.tables.get(ix))
                    .map(|table| table.key.clone())
                {
                    workspace.archive_table(&key, archived, cx);
                }
            }
            Row::Project(_) | Row::Archive(_) => {}
        });
    }

    /// The field, in the row's place. It carries its own press: `TextField`
    /// does not focus itself, and a press that reached the row would open what
    /// is being named out from under the name.
    pub(crate) fn name_field(&self, cx: &mut Context<Self>) -> AnyElement {
        div()
            .flex_1()
            .min_w_0()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, window, cx| {
                    cx.stop_propagation();
                    window.focus(&this.name_field.read(cx).focus_handle(cx), cx);
                }),
            )
            // Pressing anywhere else is finishing, not abandoning — the name
            // typed is the name meant. `escape` is what discards.
            .on_mouse_down_out(cx.listener(|this, _, window, cx| {
                this.commit_name(&CommitName, window, cx);
            }))
            .child(self.name_field.clone())
            .into_any_element()
    }

    fn start_rename(&mut self, what: Renaming, window: &mut Window, cx: &mut Context<Self>) {
        let workspace = self.workspace.read(cx);
        let label = match &what {
            Renaming::Session(id) => workspace
                .session(*id)
                .map(ChatSession::label)
                .unwrap_or_default(),
            Renaming::Board(path) => workspace
                .board_at(path)
                .map(|board| board.name.clone())
                .unwrap_or_default(),
            Renaming::Article(path) => workspace
                .projects
                .iter()
                .flat_map(|open| open.articles.iter())
                .find(|article| article.path == *path)
                .map(|article| article.title.clone())
                .unwrap_or_default(),
            Renaming::Table(key) => workspace
                .projects
                .iter()
                .flat_map(|open| open.tables.iter())
                .find(|table| table.key == *key)
                .map(|table| table.name.clone())
                .unwrap_or_default(),
        };
        self.name_field
            .update(cx, |field, cx| field.set_content(label, cx));
        self.renaming = Some(what);
        window.focus(&self.name_field.read(cx).focus_handle(cx), cx);
        cx.notify();
    }

    pub(crate) fn commit_name(&mut self, _: &CommitName, _: &mut Window, cx: &mut Context<Self>) {
        let Some(what) = self.renaming.take() else {
            return;
        };
        let name = self.name_field.read(cx).content().to_string();
        self.workspace.update(cx, |workspace, cx| match what {
            Renaming::Session(id) => workspace.rename_session(id, name, cx),
            Renaming::Board(path) => workspace.rename_board(&path, name, cx),
            Renaming::Article(path) => workspace.rename_article(&path, name, cx),
            Renaming::Table(key) => workspace.rename_table(&key, name, cx),
        });
        cx.notify();
    }

    pub(crate) fn dismiss_name(&mut self, _: &DismissName, _: &mut Window, cx: &mut Context<Self>) {
        self.renaming = None;
        cx.notify();
    }
}
