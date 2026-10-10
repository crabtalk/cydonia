//! The library: every open project's entries, one kind a tab with that kind's
//! columns, with a search, a sort and filters on its headings, and a selection
//! to act on together.
//!
//! The window's rather than a pane's: it stands over the detail column in
//! place of whatever the panes show, and is put away by going to any entry —
//! see [`Cydonia::enter_member`].

use crate::{
    model::{settings::Features, workspace::Showing},
    view::{
        component::menu::{self, Menu},
        confirm::Doomed,
        root::Cydonia,
        sidebar::{self, Row},
        stamp,
    },
};
use bezel::{
    gpui::{
        self, AnyElement, App, Context, Div, Entity, Pixels, SharedString, Stateful,
        UniformListScrollHandle, Window, div, prelude::*, px, uniform_list,
    },
    theme::{TextStyle, Theme, Typeset},
    ui::{
        icons::{self, Icon},
        input::{FieldEvent, TextField},
        menu::Item as MenuItem,
        popover,
        scroll::{self as scrollbars, Axes},
        table, tabs,
        widgets::{ButtonStyle, Buttons, Controls},
    },
};
use std::{cell::Cell, collections::HashSet, ops::Range, path::PathBuf, rc::Rc};

mod labels;

/// The search field's width, and what the bar puts between its parts.
const SEARCH: f32 = 240.;
const BAR_GAP: f32 = 12.;

/// Every list row is drawn at this height: the list lays them out at one
/// extent.
const ROW: f32 = 40.;

/// The hover group a row's checkbox and `···` are revealed by.
const ROW_GROUP: &str = "library-row";

/// Which kind the library is listing. One kind a tab, so each lists the
/// columns its kind has.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Tab {
    Articles,
    Boards,
    Tables,
    Sessions,
}

impl Tab {
    const ALL: [Self; 4] = [Self::Articles, Self::Boards, Self::Tables, Self::Sessions];

    fn label(self) -> &'static str {
        match self {
            Self::Articles => "Articles",
            Self::Boards => "Boards",
            Self::Tables => "Tables",
            Self::Sessions => "Sessions",
        }
    }

    fn icon(self) -> Icon {
        match self {
            Self::Articles => icons::files::FileText.into(),
            Self::Boards => icons::development::SquareKanban.into(),
            Self::Tables => icons::files::Table2.into(),
            Self::Sessions => icons::social::MessageCircle.into(),
        }
    }

    /// Whether the kind this tab lists is switched on.
    fn shown(self, features: &Features) -> bool {
        match self {
            Self::Articles => true,
            Self::Boards => features.boards,
            Self::Tables => features.tables,
            Self::Sessions => features.sessions,
        }
    }

    /// The columns, in order. The first three and Project are every tab's.
    fn fields(self) -> &'static [Field] {
        use Field::*;
        match self {
            Self::Articles => &[
                Check, Name, Ref, Project, Labels, Created, Edited, Status, Actions,
            ],
            Self::Boards => &[Check, Name, Ref, Project, Edited, Status, Actions],
            Self::Tables => &[Check, Name, Ref, Project, Created, Edited, Status, Actions],
            Self::Sessions => &[Check, Name, Ref, Project, Agent, Edited, Status, Actions],
        }
    }

    /// `n` of what this tab lists, as a count reads: `1 article`, `3 boards`.
    fn count(self, n: usize, archived: bool) -> String {
        let noun = match (self, n) {
            (Self::Articles, 1) => "article",
            (Self::Articles, _) => "articles",
            (Self::Boards, 1) => "board",
            (Self::Boards, _) => "boards",
            (Self::Tables, 1) => "table",
            (Self::Tables, _) => "tables",
            (Self::Sessions, 1) => "session",
            (Self::Sessions, _) => "sessions",
        };
        match archived {
            true => format!("{n} archived {noun}"),
            false => format!("{n} {noun}"),
        }
    }

    /// Whether this tab shows labels, and so is filtered and labelled by them.
    fn labelled(self) -> bool {
        self.fields().contains(&Field::Labels)
    }
}

/// One column of the library, by what it shows.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Field {
    Check,
    Name,
    /// `#12`, or a board's key.
    Ref,
    Project,
    Labels,
    Created,
    Edited,
    /// The agent a session runs on.
    Agent,
    /// Whether it is archived.
    Status,
    Actions,
}

impl Field {
    /// Its heading and width. A flexible column's weight is the least width
    /// it reads at, so at the table's least width each is exactly that.
    fn column(self) -> table::Column {
        let (title, width) = match self {
            Self::Check => ("", table::Width::Fixed(px(40.))),
            Self::Name => ("Name", table::Width::Flex(240.)),
            Self::Ref => ("Ref", table::Width::Fixed(px(88.))),
            Self::Project => ("Project", table::Width::Flex(140.)),
            Self::Labels => ("Labels", table::Width::Flex(160.)),
            Self::Created => ("Created", table::Width::Fixed(px(120.))),
            Self::Edited => ("Edited", table::Width::Fixed(px(120.))),
            Self::Agent => ("Agent", table::Width::Flex(140.)),
            Self::Status => ("Status", table::Width::Fixed(px(100.))),
            Self::Actions => ("", table::Width::Fixed(px(44.))),
        };
        table::Column::new(title, width)
    }

    fn sortable(self) -> bool {
        matches!(
            self,
            Self::Name | Self::Ref | Self::Project | Self::Created | Self::Edited | Self::Agent
        )
    }
}

/// Which of a tab's entries are listed, by whether they are put away.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Status {
    Active,
    Archived,
    All,
}

impl Status {
    const ALL: [Self; 3] = [Self::Active, Self::Archived, Self::All];

    fn label(self) -> &'static str {
        match self {
            Self::Active => "Active",
            Self::Archived => "Archived",
            Self::All => "All",
        }
    }

    fn keeps(self, archived: bool) -> bool {
        match self {
            Self::Active => !archived,
            Self::Archived => archived,
            Self::All => true,
        }
    }
}

/// What the listings are ordered by.
#[derive(Clone, Copy, PartialEq)]
struct Order {
    field: Field,
    ascending: bool,
}

impl Order {
    /// Newest first, which every tab has.
    const EDITED: Self = Self {
        field: Field::Edited,
        ascending: false,
    };
}

/// What the library was narrowed to — what going back to it puts back.
#[derive(Clone, PartialEq)]
pub(crate) struct Shelf {
    tab: Tab,
    status: Status,
    project: Option<PathBuf>,
    agent: Option<String>,
    labels: Vec<String>,
    order: Order,
    query: String,
}

/// What the library is showing, while it is up.
pub(crate) struct Library {
    tab: Tab,
    status: Status,
    /// The one project listed, by its path, or every open one.
    project: Option<PathBuf>,
    /// The one agent whose sessions are listed, by name, or every one.
    agent: Option<String>,
    /// The labels listed, any one of them; every listing while empty.
    pub(crate) labels: Vec<String>,
    order: Order,
    query: Entity<TextField>,
    selected: HashSet<Row>,
    scroll: UniformListScrollHandle,
    /// How wide the bar was laid out last frame, and the tabs laid out in a
    /// row the last time they were: the tabs give way to a select when the
    /// two do not fit beside the search.
    bar: Rc<Cell<Pixels>>,
    strip: Rc<Cell<Pixels>>,
}

impl Library {
    /// Stop filtering on `name`, a label that is gone.
    pub(crate) fn drop_label_filter(&mut self, name: &str) {
        self.labels.retain(|label| label != name);
    }

    /// What this library is narrowed to.
    pub(crate) fn shelf(&self, cx: &App) -> Shelf {
        Shelf {
            tab: self.tab,
            status: self.status,
            project: self.project.clone(),
            agent: self.agent.clone(),
            labels: self.labels.clone(),
            order: self.order,
            query: self.query.read(cx).content().to_string(),
        }
    }

    /// A library narrowed as `shelf` says.
    fn shelved(shelf: Shelf, cx: &mut Context<Cydonia>) -> Self {
        let library = Self::new(cx);
        library
            .query
            .update(cx, |field, cx| field.set_content(shelf.query, cx));
        Self {
            tab: shelf.tab,
            status: shelf.status,
            project: shelf.project,
            agent: shelf.agent,
            labels: shelf.labels,
            order: shelf.order,
            ..library
        }
    }

    fn new(cx: &mut Context<Cydonia>) -> Self {
        let query = cx.new(|cx| {
            TextField::new(cx)
                .with_frame(false)
                .with_placeholder("Search the library…")
        });
        // The list is drawn from the root's render, so what is typed has to
        // reach the root: the field's own `notify` repaints the field alone.
        cx.subscribe(&query, |_, _, event: &FieldEvent, cx| {
            if matches!(event, FieldEvent::Changed(_)) {
                cx.notify();
            }
        })
        .detach();
        Self {
            tab: Tab::Articles,
            status: Status::Active,
            project: None,
            agent: None,
            labels: Vec::new(),
            order: Order::EDITED,
            query,
            selected: HashSet::new(),
            scroll: UniformListScrollHandle::new(),
            bar: Rc::new(Cell::new(px(0.))),
            strip: Rc::new(Cell::new(px(0.))),
        }
    }
}

/// One listing as it is drawn, read off the workspace for the frame.
struct Listing {
    row: Row,
    title: SharedString,
    reference: Option<SharedString>,
    /// What [`Listing::reference`] counts, where it is a number: what Ref
    /// sorts on, so `#9` comes before `#10`.
    number: Option<u64>,
    project: SharedString,
    agent: Option<SharedString>,
    /// Milliseconds, as are [`Listing::created`].
    touched: u128,
    created: Option<u128>,
    archived: bool,
    /// `None` where the tab shows none.
    labels: Option<Vec<String>>,
}

impl Cydonia {
    /// Put the library up, or take it down.
    pub(crate) fn toggle_library(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        match self.library.is_some() {
            true => {
                self.commit(cx);
                self.library = None;
                cx.notify();
            }
            false => self.put_up_library(Library::new(cx), window, cx),
        }
    }

    /// Put the library up narrowed as `shelf` says — where the history goes
    /// back to it.
    pub(crate) fn open_library(
        &mut self,
        shelf: Shelf,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.put_up_library(Library::shelved(shelf, cx), window, cx);
    }

    /// Put the library up on the articles in `project` carrying `label`.
    pub(crate) fn library_on_label(
        &mut self,
        project: PathBuf,
        label: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let library = Library {
            project: Some(project),
            labels: vec![label],
            ..Library::new(cx)
        };
        self.put_up_library(library, window, cx);
    }

    fn put_up_library(&mut self, library: Library, window: &mut Window, cx: &mut Context<Self>) {
        self.commit(cx);
        self.library = Some(library);
        // The panes under it are not drawn, and a focus left in one reaches
        // none of the window's commands.
        window.focus(&self.focus, cx);
        cx.notify();
    }

    /// The sidebar's row that puts the library up, lit while it is.
    pub(crate) fn library_entry(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = Theme::of(cx).clone();
        let open = self.library.is_some();
        let tone = sidebar::tint(open, false, &theme);
        sidebar::row("library-entry", "library-entry", open, false, 0, &theme)
            .flex_none()
            .mb(px(4.))
            .child(
                icons::icon(icons::text::LibraryBig)
                    .size(px(14.))
                    .text_color(tone),
            )
            .child(
                div()
                    .flex_1()
                    .text_style(TextStyle::Body)
                    .text_color(tone)
                    .child("Library"),
            )
            .on_click(cx.listener(|this, _, window, cx| this.toggle_library(window, cx)))
            .into_any_element()
    }

    /// Every entry of the tab's kind in every open project, or the one picked,
    /// with what a switch hides left out.
    fn listings(&self, cx: &App) -> Vec<Listing> {
        let Some(library) = &self.library else {
            return Vec::new();
        };
        let workspace = self.workspace.read(cx);
        let features = &workspace.settings.features;
        let mut out = Vec::new();
        for open in &workspace.projects {
            if library
                .project
                .as_ref()
                .is_some_and(|picked| *picked != open.path)
            {
                continue;
            }
            let project = SharedString::from(open.name());
            let number = |number: Option<u64>| number.map(|n| SharedString::from(format!("#{n}")));
            let entry = |showing: Showing, title: String, reference, touched, archived| {
                let row = Row::Entry {
                    project: open.path.clone(),
                    showing,
                };
                if !sidebar::shown(&row, features) {
                    return None;
                }
                Some(Listing {
                    row,
                    title: title.into(),
                    reference,
                    number: None,
                    project: project.clone(),
                    agent: None,
                    touched,
                    created: None,
                    archived,
                    labels: None,
                })
            };
            match library.tab {
                Tab::Articles => {
                    for article in &open.articles {
                        if let Some(mut listing) = entry(
                            Showing::Article(article.id.clone()),
                            article.label().to_owned(),
                            number(article.number),
                            article.touched,
                            article.archived,
                        ) {
                            listing.number = article.number;
                            listing.created = article.created;
                            listing.labels = Some(article.labels.clone());
                            out.push(listing);
                        }
                    }
                }
                Tab::Boards => {
                    for board in &open.boards {
                        out.extend(entry(
                            Showing::Board(board.id.clone()),
                            board.label().to_owned(),
                            Some(board.key.clone().into()),
                            board.touched,
                            board.archived,
                        ));
                    }
                }
                Tab::Tables => {
                    // The store keeps seconds; every other stamp is milliseconds.
                    let ms = |seconds: i64| seconds.max(0) as u128 * 1000;
                    for table in &open.tables {
                        if let Some(mut listing) = entry(
                            Showing::Table(table.key.clone()),
                            table.name.clone(),
                            number(table.number),
                            ms(table.updated_at.unwrap_or(table.created_at)),
                            table.archived,
                        ) {
                            listing.number = table.number;
                            listing.created = Some(ms(table.created_at));
                            out.push(listing);
                        }
                    }
                }
                Tab::Sessions => {
                    for chat in &open.sessions {
                        if let Some(mut listing) = entry(
                            Showing::Session(chat.id),
                            chat.label(),
                            number(chat.number),
                            chat.touched(),
                            chat.closed,
                        ) {
                            listing.number = chat.number;
                            listing.agent = Some(chat.entry.name.clone().into());
                            listing.labels = Some(chat.labels.clone());
                            out.push(listing);
                        }
                    }
                }
            }
        }
        out
    }

    /// What the open tab, the query and the sort leave, in order.
    ///
    /// With how many the tab holds before the search and the label filter, to
    /// say how many those hide.
    fn library_listings(&self, cx: &App) -> (Vec<Listing>, usize) {
        let Some(library) = &self.library else {
            return (Vec::new(), 0);
        };
        let labelled = library.tab.labelled();
        let held: Vec<Listing> = self
            .listings(cx)
            .into_iter()
            .filter(|listing| library.status.keeps(listing.archived))
            .filter(|listing| {
                library.agent.is_none()
                    || !library.tab.fields().contains(&Field::Agent)
                    || listing.agent.as_deref() == library.agent.as_deref()
            })
            .collect();
        let total = held.len();
        let mut listings: Vec<Listing> = held
            .into_iter()
            .filter(|listing| {
                !labelled || labels::passes(listing.labels.as_deref(), &library.labels)
            })
            .collect();
        let query = library.query.read(cx).content().trim().to_owned();
        if !query.is_empty() {
            let labels: Vec<String> = listings
                .iter()
                .map(|listing| match &listing.reference {
                    Some(reference) => format!("{} {reference}", listing.title),
                    None => listing.title.to_string(),
                })
                .collect();
            let kept: HashSet<usize> = popover::filter_indices(&query, &labels)
                .into_iter()
                .collect();
            listings = listings
                .into_iter()
                .enumerate()
                .filter(|(ix, _)| kept.contains(ix))
                .map(|(_, listing)| listing)
                .collect();
        }
        let sort = library.order;
        let folded =
            |text: &Option<SharedString>| text.as_deref().unwrap_or_default().to_lowercase();
        listings.sort_by(|a, b| {
            // Nothing to name it by yet — a session before its first turn —
            // is last whichever way the column runs.
            if sort.field == Field::Ref {
                let missing = a.reference.is_none().cmp(&b.reference.is_none());
                if missing.is_ne() {
                    return missing;
                }
            }
            let order = match sort.field {
                Field::Ref => match (a.number, b.number) {
                    (Some(a), Some(b)) => a.cmp(&b),
                    _ => folded(&a.reference).cmp(&folded(&b.reference)),
                },
                Field::Name => a.title.to_lowercase().cmp(&b.title.to_lowercase()),
                Field::Project => a.project.cmp(&b.project),
                Field::Agent => folded(&a.agent).cmp(&folded(&b.agent)),
                Field::Created => a.created.cmp(&b.created),
                _ => a.touched.cmp(&b.touched),
            };
            match sort.ascending {
                true => order,
                false => order.reverse(),
            }
        });
        (listings, total)
    }

    // ── state ────────────────────────────────────────────────────

    fn library_tab(&mut self, tab: Tab, cx: &mut Context<Self>) {
        if let Some(library) = &mut self.library {
            library.tab = tab;
            library.selected.clear();
            // An order by a column the tab does not have would be one nobody
            // can see or undo.
            if !tab.fields().contains(&library.order.field) {
                library.order = Order::EDITED;
            }
            cx.notify();
        }
    }

    fn library_agent(&mut self, agent: Option<String>, cx: &mut Context<Self>) {
        if let Some(library) = &mut self.library {
            library.agent = agent;
            library.selected.clear();
            cx.notify();
        }
    }

    fn library_status(&mut self, status: Status, cx: &mut Context<Self>) {
        if let Some(library) = &mut self.library {
            library.status = status;
            library.selected.clear();
            cx.notify();
        }
    }

    fn library_project(&mut self, project: Option<PathBuf>, cx: &mut Context<Self>) {
        if let Some(library) = &mut self.library {
            library.project = project;
            library.selected.clear();
            cx.notify();
        }
    }

    fn library_sort_by(&mut self, field: Field, ascending: bool, cx: &mut Context<Self>) {
        if let Some(library) = &mut self.library {
            library.order = Order { field, ascending };
            cx.notify();
        }
    }

    /// A press on `field`'s heading: the sorted column reverses, any other
    /// starts ascending.
    fn library_sort(&mut self, field: Field, cx: &mut Context<Self>) {
        if let Some(library) = &mut self.library {
            library.order = Order {
                field,
                ascending: library.order.field != field || !library.order.ascending,
            };
            cx.notify();
        }
    }

    fn library_select(&mut self, row: Row, cx: &mut Context<Self>) {
        if let Some(library) = &mut self.library {
            if !library.selected.remove(&row) {
                library.selected.insert(row);
            }
            cx.notify();
        }
    }

    /// Take the selection, leaving none.
    fn take_selected(&mut self) -> Vec<Row> {
        self.library
            .as_mut()
            .map(|library| library.selected.drain().collect())
            .unwrap_or_default()
    }

    /// The entries in the selection.
    pub(crate) fn selected_rows(&self) -> Vec<Row> {
        self.library
            .iter()
            .flat_map(|library| library.selected.iter().cloned())
            .collect()
    }

    // ── acting on the selection ──────────────────────────────────

    /// Put every selected entry away, or bring every one back.
    fn archive_selected(&mut self, archived: bool, window: &mut Window, cx: &mut Context<Self>) {
        let rows = self.selected_rows();
        self.take_selected();
        for row in &rows {
            self.archive_entry(row, archived, window, cx);
        }
        cx.notify();
    }

    /// Move every selected article into the project at `to`, its folder and
    /// pictures with it. Only articles move between projects.
    fn move_selected(&mut self, to: usize, cx: &mut Context<Self>) {
        let rows = self.selected_rows();
        self.take_selected();
        for row in rows {
            if !matches!(
                row,
                Row::Entry {
                    showing: Showing::Article(_),
                    ..
                }
            ) {
                continue;
            }
            // Read for each in turn: a move takes the article out of its
            // project's list, and every one after it shifts up a place.
            if let Some((from, ix)) = self.located(&row, cx)
                && from != to
            {
                self.workspace
                    .update(cx, |workspace, cx| workspace.move_article(from, ix, to, cx));
            }
        }
        cx.notify();
    }

    /// Ask before deleting everything selected.
    fn delete_selected(&mut self, cx: &mut Context<Self>) {
        let doomed = self
            .take_selected()
            .into_iter()
            .map(Doomed::Entry)
            .collect();
        self.ask_delete_many(doomed, cx);
    }

    /// Open a listing's entry in the drawer over the library. A session before
    /// its first turn has no number to name it by, and opens where it lives,
    /// which puts the library away.
    fn open_listing(&mut self, row: &Row, window: &mut Window, cx: &mut Context<Self>) {
        match self.drawn_reference(row, cx) {
            Some(reference) => self.peek(None, &reference, window, cx),
            None => self.open_row(row, window, cx),
        }
    }

    /// How an entry is named to the drawer, `project#12`. Nothing for a
    /// session before its first turn, which has no number yet.
    fn drawn_reference(&self, row: &Row, cx: &App) -> Option<String> {
        let Row::Entry { project, showing } = row else {
            return None;
        };
        let workspace = self.workspace.read(cx);
        let open = &workspace.projects[workspace.project_at(project)?];
        let number = match showing {
            Showing::Session(id) => open.session(*id)?.number,
            Showing::Article(id) => open.articles.get(open.article_ix(id)?)?.number,
            Showing::Board(id) => open.boards.get(open.board_ix(id)?)?.number,
            Showing::Table(key) => open.tables.iter().find(|table| &table.key == key)?.number,
        }?;
        Some(format!(
            "{}#{number}",
            project.file_name()?.to_string_lossy()
        ))
    }

    // ── chrome ───────────────────────────────────────────────────

    /// The library, in the detail column's place.
    pub(crate) fn library_view(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = Theme::of(cx).clone();
        let Some(library) = &self.library else {
            return div().into_any_element();
        };
        let selected = !library.selected.is_empty();
        let (listings, total) = self.library_listings(cx);
        let archived = library.status == Status::Archived;
        let count = match listings.len() == total {
            true => library.tab.count(total, archived),
            false => format!(
                "{} of {}",
                listings.len(),
                library.tab.count(total, archived)
            ),
        };
        // Unarchive where everything selected that is listed is put away.
        let mut picked = listings
            .iter()
            .filter(|listing| library.selected.contains(&listing.row))
            .peekable();
        let unarchive = picked.peek().is_some() && picked.all(|listing| listing.archived);
        let listings = Rc::new(listings);
        let bar = match selected {
            true => self.library_selection_bar(unarchive, window, cx),
            false => self.library_bar(window, cx),
        };
        let filters = self.filter_strip(cx);
        let body = match listings.is_empty() {
            true => div()
                .flex_1()
                .flex()
                .items_center()
                .justify_center()
                .text_style(TextStyle::Callout)
                .text_color(theme.text_faint)
                .child("Nothing here")
                .into_any_element(),
            false => self.library_list(listings, window, cx),
        };
        div()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .px(px(24.))
            .pt(px(16.))
            .pb(px(20.))
            .gap(px(12.))
            .child(
                div()
                    .flex_none()
                    .h(px(Theme::BUTTON_HEIGHT + 4.))
                    .flex()
                    .flex_row()
                    .items_center()
                    .child(bar),
            )
            .children(filters)
            .child(body)
            .child(
                div()
                    .flex_none()
                    .text_style(TextStyle::Caption)
                    .text_color(theme.text_muted)
                    .child(count),
            )
            .into_any_element()
    }

    /// The tabs and the search: what the library is narrowed to while nothing
    /// is selected.
    fn library_bar(&self, window: &mut Window, cx: &mut Context<Self>) -> Div {
        let theme = Theme::of(cx).clone();
        let Some(library) = &self.library else {
            return div();
        };
        let tab = library.tab;
        let (bar, strip) = (library.bar.clone(), library.strip.clone());
        let features = &self.workspace.read(cx).settings.features;
        let shown: Vec<Tab> = Tab::ALL
            .into_iter()
            .filter(|at| at.shown(features))
            .collect();
        // Measured while the tabs are a row, and kept while they are not: the
        // row is what has to fit for it to come back. A row never measured is
        // drawn, so that it is.
        let wide = f32::from(strip.get());
        let fits = wide == 0. || f32::from(bar.get()) >= wide + BAR_GAP + SEARCH;
        let tabs = match fits {
            true => div()
                .flex_none()
                .relative()
                .flex()
                .flex_row()
                .items_center()
                .gap(px(2.))
                .children(shown.into_iter().map(|at| {
                    let state = match at == tab {
                        true => tabs::State::Focused,
                        false => tabs::State::Resting,
                    };
                    tabs::tab(
                        &theme,
                        format!("library-{}", at.label()),
                        tabs::Label::new(at.label()),
                        state,
                    )
                    .on_click(cx.listener(move |this, _, _, cx| this.library_tab(at, cx)))
                }))
                .child(measure(strip))
                .into_any_element(),
            false => self.tab_picker(tab, shown, window, cx).into_any_element(),
        };
        let search = div()
            .w(px(SEARCH))
            .flex_none()
            .flex()
            .flex_row()
            .items_center()
            .gap(px(6.))
            .px(px(8.))
            .h(px(Theme::BUTTON_HEIGHT))
            .rounded(px(Theme::control_radius()))
            .border_1()
            .border_color(theme.border)
            .child(
                icons::icon(icons::text::Search)
                    .size(px(13.))
                    .flex_none()
                    .text_color(theme.text_faint),
            )
            .child(div().flex_1().min_w_0().child(library.query.clone()));
        div()
            .flex_1()
            .min_w_0()
            .relative()
            .flex()
            .flex_row()
            .items_center()
            .gap(px(BAR_GAP))
            .child(tabs)
            .child(div().flex_1())
            .child(search)
            .child(measure(bar))
    }

    /// The tabs as a select, for a bar too narrow to hold them in a row.
    fn tab_picker(
        &self,
        tab: Tab,
        shown: Vec<Tab>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let theme = Theme::of(cx).clone();
        let menu = Menu::LibraryTab;
        let trigger = theme
            .select_trigger(tab.label())
            .flex_none()
            .id("library-tab")
            .relative()
            .on_click(cx.listener({
                let menu = menu.clone();
                move |this, _, _, cx| {
                    cx.stop_propagation();
                    this.toggle_menu(menu.clone(), cx);
                }
            }));
        let card = (self.menu == Some(Menu::LibraryTab)).then(|| {
            let rows = shown
                .into_iter()
                .map(|at| {
                    menu::row(
                        MenuItem::action(at.label()).checked(at == tab),
                        move |this, _, cx| this.library_tab(at, cx),
                    )
                })
                .collect();
            let id = SharedString::from("library-tab-menu");
            popover::anchored_menu_below(id.clone(), self.menu_card(id, rows, window, cx), None)
        });
        self.menu_press(trigger, menu, cx).children(card)
    }

    /// What the selection can be done to, in the bar's place while there is
    /// one.
    fn library_selection_bar(
        &self,
        unarchive: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Div {
        let theme = Theme::of(cx).clone();
        let Some(library) = &self.library else {
            return div();
        };
        let count = library.selected.len();
        let articles = library.tab == Tab::Articles;
        let labelled = library.tab.labelled();
        let projects = self.workspace.read(cx).projects.len();
        div()
            .flex_1()
            .min_w_0()
            .flex()
            .flex_row()
            .items_center()
            .gap(px(8.))
            .child(
                div()
                    .text_style(TextStyle::Callout)
                    .text_color(theme.text)
                    .child(format!("{count} selected")),
            )
            .child(
                theme
                    .button(
                        match unarchive {
                            true => "Unarchive",
                            false => "Archive",
                        },
                        ButtonStyle::Ghost,
                        None,
                    )
                    .id("library-archive")
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.archive_selected(!unarchive, window, cx)
                    })),
            )
            .when(articles && projects > 1, |bar| {
                bar.child(
                    self.library_trigger("library-move", "Move to", Menu::LibraryMove, cx)
                        .children(self.move_menu(window, cx)),
                )
            })
            .when(labelled, |bar| bar.child(self.label_button(cx)))
            .child(
                theme
                    .button("Delete", ButtonStyle::Ghost, None)
                    .id("library-delete")
                    .on_click(cx.listener(|this, _, _, cx| this.delete_selected(cx))),
            )
            .child(div().flex_1())
            .child(
                theme
                    .button("Clear", ButtonStyle::Ghost, None)
                    .id("library-clear")
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.take_selected();
                        cx.notify();
                    })),
            )
    }

    /// A labelled button that opens `menu` under itself.
    fn library_trigger(
        &self,
        id: &'static str,
        label: &'static str,
        menu: Menu,
        cx: &Context<Self>,
    ) -> Stateful<Div> {
        let theme = Theme::of(cx);
        let button = theme
            .button(label, ButtonStyle::Ghost, None)
            .id(id)
            .relative()
            .on_click(cx.listener({
                let menu = menu.clone();
                move |this, _, _, cx| {
                    cx.stop_propagation();
                    this.toggle_menu(menu.clone(), cx);
                }
            }));
        self.menu_press(button, menu, cx)
    }

    /// A heading that opens `menu`, drawn under it as `card` while it is
    /// open: a filter on the column, and its sort where it has one.
    fn heading_picker(
        &self,
        column: &table::Column,
        sorted: Option<bool>,
        id: &'static str,
        menu: Menu,
        card: Option<AnyElement>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = Theme::of(cx).clone();
        let cell = table::header_cell(&theme, column, sorted)
            .child(
                icons::icon(icons::arrows::ChevronDown)
                    .size(px(11.))
                    .text_color(theme.text_muted),
            )
            .id(id)
            .relative()
            .on_click(cx.listener({
                let menu = menu.clone();
                move |this, _, _, cx| {
                    cx.stop_propagation();
                    this.toggle_menu(menu.clone(), cx);
                }
            }));
        self.menu_press(cell, menu, cx)
            .children(card)
            .into_any_element()
    }

    /// The menu under a sortable heading's picker: its two sorts, then a line.
    fn sorts(&self, field: Field) -> Vec<(MenuItem, menu::Act)> {
        let sorted = self
            .library
            .as_ref()
            .filter(|library| library.order.field == field)
            .map(|library| library.order.ascending);
        vec![
            menu::row(
                MenuItem::action("Sort A → Z")
                    .with_icon(icons::arrows::ArrowDownAZ)
                    .checked(sorted == Some(true)),
                move |this, _, cx| this.library_sort_by(field, true, cx),
            ),
            menu::row(
                MenuItem::action("Sort Z → A")
                    .with_icon(icons::arrows::ArrowDownZA)
                    .checked(sorted == Some(false)),
                move |this, _, cx| this.library_sort_by(field, false, cx),
            ),
            menu::row(MenuItem::Separator, |_, _, _| {}),
        ]
    }

    fn library_project_menu(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if self.menu != Some(Menu::LibraryProject) {
            return None;
        }
        let Some(library) = &self.library else {
            return None;
        };
        let picked = library.project.clone();
        let mut rows = self.sorts(Field::Project);
        rows.push(menu::row(
            MenuItem::action("All projects")
                .with_icon(icons::files::Folders)
                .checked(picked.is_none()),
            |this, _, cx| this.library_project(None, cx),
        ));
        rows.extend(self.workspace.read(cx).projects.iter().map(|open| {
            let path = open.path.clone();
            menu::row(
                MenuItem::action(open.name())
                    .with_icon(icons::files::Folder)
                    .checked(picked.as_ref() == Some(&path)),
                move |this, _, cx| this.library_project(Some(path.clone()), cx),
            )
        }));
        let id = SharedString::from("library-project-menu");
        Some(popover::anchored_menu_below(
            id.clone(),
            self.menu_card(id, rows, window, cx),
            None,
        ))
    }

    /// The projects a selection's articles can move into.
    fn move_menu(&self, window: &mut Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        if self.menu != Some(Menu::LibraryMove) {
            return None;
        }
        let rows = self
            .workspace
            .read(cx)
            .projects
            .iter()
            .enumerate()
            .map(|(to, open)| {
                menu::row(
                    MenuItem::action(open.name()).with_icon(icons::files::Folder),
                    move |this, _, cx| this.move_selected(to, cx),
                )
            })
            .collect();
        let id = SharedString::from("library-move-menu");
        Some(popover::anchored_menu_below(
            id.clone(),
            self.menu_card(id, rows, window, cx),
            None,
        ))
    }

    /// The agents a session in an open project runs on, to list one's alone.
    fn agent_menu(&self, window: &mut Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        if self.menu != Some(Menu::LibraryAgent) {
            return None;
        }
        let picked = self.library.as_ref()?.agent.clone();
        let mut names: Vec<String> = self
            .workspace
            .read(cx)
            .projects
            .iter()
            .flat_map(|open| &open.sessions)
            .map(|chat| chat.entry.name.clone())
            .collect();
        names.sort();
        names.dedup();
        let mut rows = self.sorts(Field::Agent);
        rows.push(menu::row(
            MenuItem::action("All agents").checked(picked.is_none()),
            |this, _, cx| this.library_agent(None, cx),
        ));
        rows.extend(names.into_iter().map(|name| {
            let checked = picked.as_ref() == Some(&name);
            menu::row(
                MenuItem::action(name.clone()).checked(checked),
                move |this, _, cx| this.library_agent(Some(name.clone()), cx),
            )
        }));
        let id = SharedString::from("library-agent-menu");
        Some(popover::anchored_menu_below(
            id.clone(),
            self.menu_card(id, rows, window, cx),
            None,
        ))
    }

    fn status_menu(&self, window: &mut Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        if self.menu != Some(Menu::LibraryStatus) {
            return None;
        }
        let picked = self.library.as_ref()?.status;
        let rows = Status::ALL
            .into_iter()
            .map(|status| {
                menu::row(
                    MenuItem::action(status.label()).checked(status == picked),
                    move |this, _, cx| this.library_status(status, cx),
                )
            })
            .collect();
        let id = SharedString::from("library-status-menu");
        Some(popover::anchored_menu_below(
            id.clone(),
            self.menu_card(id, rows, window, cx),
            None,
        ))
    }

    /// The listings as rows under one heading.
    fn library_list(
        &self,
        listings: Rc<Vec<Listing>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = Theme::of(cx).clone();
        let Some(library) = &self.library else {
            return div().into_any_element();
        };
        let (order, scroll, tab) = (library.order, library.scroll.clone(), library.tab);
        let fields = tab.fields();
        let columns: Rc<Vec<table::Column>> =
            Rc::new(fields.iter().map(|field| field.column()).collect());
        let width = least(&columns);
        let headings: Vec<AnyElement> = fields
            .iter()
            .zip(columns.iter())
            .enumerate()
            .map(|(ix, (&field, column))| {
                let sorted = (field.sortable() && order.field == field).then_some(order.ascending);
                match field {
                    Field::Project => {
                        let card = self.library_project_menu(window, cx);
                        return self.heading_picker(
                            column,
                            sorted,
                            "library-heading-project",
                            Menu::LibraryProject,
                            card,
                            cx,
                        );
                    }
                    Field::Agent => {
                        let card = self.agent_menu(window, cx);
                        return self.heading_picker(
                            column,
                            sorted,
                            "library-heading-agent",
                            Menu::LibraryAgent,
                            card,
                            cx,
                        );
                    }
                    Field::Status => {
                        let card = self.status_menu(window, cx);
                        return self.heading_picker(
                            column,
                            None,
                            "library-heading-status",
                            Menu::LibraryStatus,
                            card,
                            cx,
                        );
                    }
                    Field::Labels => return self.labels_heading(column, cx),
                    _ => {}
                }
                let cell = table::header_cell(&theme, column, sorted).id(("library-heading", ix));
                match field.sortable() {
                    true => cell
                        .on_click(cx.listener(move |this, _, _, cx| this.library_sort(field, cx)))
                        .into_any_element(),
                    false => cell.cursor_default().into_any_element(),
                }
            })
            .collect();
        let rows = uniform_list(
            "library-list",
            listings.len(),
            cx.processor(move |this, range: Range<usize>, window, cx| {
                range
                    .map(|ix| this.library_row(&listings[ix], ix, tab, &columns, window, cx))
                    .collect::<Vec<_>>()
            }),
        )
        .track_scroll(&scroll)
        .flex_1()
        .min_h_0();
        div()
            .id("library-table")
            .flex_1()
            .min_h_0()
            .map(|el| scrollbars::scrolls(el, Axes::Horizontal))
            .child(
                table::table(&theme)
                    .h_full()
                    .min_w(px(width))
                    .child(table::header(&theme).flex_none().children(headings))
                    .child(rows),
            )
            .into_any_element()
    }

    /// The box that picks a listing out of the selection, or puts it back. On
    /// show while anything is selected; otherwise on the row's hover.
    fn library_check(&self, listing: &Listing, ix: usize, cx: &mut Context<Self>) -> Stateful<Div> {
        let theme = Theme::of(cx).clone();
        let (checked, any) = self.library.as_ref().map_or((false, false), |library| {
            (
                library.selected.contains(&listing.row),
                !library.selected.is_empty(),
            )
        });
        theme
            .checkbox(checked)
            .id(("library-check", ix))
            .cursor_pointer()
            .when(!checked && !any, |el| {
                el.invisible().group_hover(ROW_GROUP, |el| el.visible())
            })
            .on_click(cx.listener({
                let row = listing.row.clone();
                move |this, _, _, cx| {
                    cx.stop_propagation();
                    this.library_select(row.clone(), cx);
                }
            }))
    }

    /// A listing's `···`: the sidebar's menu for its entry.
    fn library_actions(
        &self,
        listing: &Listing,
        ix: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let id = SharedString::from(format!("library-menu-{ix}"));
        let menu = Menu::Library(listing.row.clone());
        self.menu_button(
            id,
            Some(ROW_GROUP),
            icons::layout::Ellipsis,
            menu.clone(),
            cx,
        )
        .children(self.entry_menu(menu, &listing.row, listing.archived, window, cx))
        .into_any_element()
    }

    fn is_selected(&self, row: &Row) -> bool {
        self.library
            .as_ref()
            .is_some_and(|library| library.selected.contains(row))
    }

    /// One line of the list.
    fn library_row(
        &self,
        listing: &Listing,
        ix: usize,
        tab: Tab,
        columns: &[table::Column],
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = Theme::of(cx).clone();
        let checked = self.is_selected(&listing.row);
        let muted = |text: Option<SharedString>| {
            div()
                .min_w_0()
                .truncate()
                .text_color(theme.text_muted)
                .child(text.unwrap_or_default())
                .into_any_element()
        };
        let name = || {
            div()
                .min_w_0()
                .flex()
                .flex_row()
                .items_center()
                .gap(px(8.))
                .child(
                    icons::icon(tab.icon())
                        .size(px(14.))
                        .flex_none()
                        .text_color(sidebar::tint(false, listing.archived, &theme)),
                )
                .child(
                    div()
                        .min_w_0()
                        .truncate()
                        .text_color(match listing.archived {
                            true => theme.text_faint,
                            false => theme.text,
                        })
                        .child(listing.title.clone()),
                )
                .into_any_element()
        };
        let cells = tab
            .fields()
            .iter()
            .map(|field| match field {
                Field::Check => self.library_check(listing, ix, cx).into_any_element(),
                Field::Name => name(),
                Field::Ref => div()
                    .text_style(TextStyle::Caption)
                    .text_color(theme.text_faint)
                    .child(listing.reference.clone().unwrap_or_default())
                    .into_any_element(),
                Field::Project => muted(Some(listing.project.clone())),
                Field::Labels => self.label_cell(&listing.row, listing.labels.as_deref(), ix, cx),
                Field::Created => muted(listing.created.map(|at| stamp::coarse(at).into())),
                Field::Edited => muted(Some(stamp::coarse(listing.touched).into())),
                Field::Agent => muted(listing.agent.clone()),
                Field::Status => muted(Some(
                    match listing.archived {
                        true => Status::Archived,
                        false => Status::Active,
                    }
                    .label()
                    .into(),
                )),
                Field::Actions => self.library_actions(listing, ix, window, cx),
            })
            .collect();
        table::row(&theme, columns, ix == 0, checked, cells)
            .h(px(ROW))
            .id(("library-row", ix))
            .group(ROW_GROUP)
            .cursor_pointer()
            .on_click(cx.listener({
                let row = listing.row.clone();
                move |this, _, window, cx| this.open_listing(&row, window, cx)
            }))
            .into_any_element()
    }
}

/// Write the width its parent is laid out at into `into`, and draw again when
/// that moved. The parent must be `relative`.
fn measure(into: Rc<Cell<Pixels>>) -> impl IntoElement {
    gpui::canvas(
        move |bounds, window, _| {
            if into.replace(bounds.size.width) != bounds.size.width {
                window.on_next_frame(|window, _| window.refresh());
            }
        },
        |_, _, _, _| {},
    )
    .absolute()
    .size_full()
}

/// The narrowest `columns` are laid out at; a pane narrower scrolls the table
/// sideways.
fn least(columns: &[table::Column]) -> f32 {
    columns
        .iter()
        .map(|column| match column.width {
            table::Width::Fixed(width) => f32::from(width),
            table::Width::Flex(least) => least,
        })
        .sum()
}
