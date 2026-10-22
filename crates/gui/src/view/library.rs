//! The library: every open project's entries, one kind a tab with that kind's
//! columns, with a search, a sort and filters on its headings, and a selection
//! to act on together.
//!
//! The window's rather than a pane's: it stands over the detail column in
//! place of whatever the panes show, and is put away by going to any entry —
//! see [`Cydonia::enter_member`].

use crate::{
    model::{
        project::ProjectId,
        settings::Features,
        workspace::{Opening, Showing, Workspace},
    },
    view::{
        kit::{
            icon::kind_icon,
            labels::{LabelHost, Labels},
            menu::{self, Act, MenuHost},
            stamp, text,
            watch::Watch,
        },
        modals::confirm::Doomed,
        sidebar::{self, Row},
        window::{entry::Host, reach::InWindow, root::Cydonia},
    },
};
use artifact::{archive::Archived, kind::Kind, reference::Reference};
use bezel::{
    gpui::{
        self, AnyElement, App, AppContext as _, Context, Div, Entity, Pixels, SharedString,
        Stateful, Subscription, UniformListScrollHandle, WeakEntity, Window, div, prelude::*, px,
        uniform_list,
    },
    theme::{TextStyle, Theme, Typeset},
    ui::{
        context_menu::{ContextMenu, Place},
        icons::{self, Icon},
        input::{FieldEvent, TextField},
        menu::Item as MenuItem,
        popover, table, tabs,
        widgets::{ButtonStyle, Buttons, Content, Controls},
    },
};
use std::{cell::Cell, collections::HashSet, ops::Range, rc::Rc};

mod column;

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
    Apps,
}

impl Tab {
    const ALL: [Self; 5] = [
        Self::Articles,
        Self::Boards,
        Self::Tables,
        Self::Sessions,
        Self::Apps,
    ];

    fn label(self) -> &'static str {
        match self {
            Self::Articles => "Articles",
            Self::Boards => "Boards",
            Self::Tables => "Tables",
            Self::Sessions => "Sessions",
            Self::Apps => "Apps",
        }
    }

    fn kind(self) -> Kind {
        match self {
            Self::Articles => Kind::Article,
            Self::Boards => Kind::Board,
            Self::Tables => Kind::Table,
            Self::Sessions => Kind::Session,
            Self::Apps => Kind::App,
        }
    }

    fn icon(self) -> Icon {
        kind_icon(self.kind())
    }

    /// Whether the kind this tab lists is switched on.
    fn shown(self, features: &Features) -> bool {
        match self {
            Self::Articles | Self::Apps => true,
            Self::Boards => features.boards,
            Self::Tables => features.tables,
            Self::Sessions => features.sessions,
        }
    }

    /// The columns, in order. Name, Ref and Project are every entry tab's.
    fn fields(self) -> &'static [Field] {
        use Field::*;
        match self {
            Self::Articles => &[
                Check, Name, Ref, Project, Labels, Created, Edited, Status, Actions,
            ],
            Self::Boards => &[Check, Name, Ref, Project, Edited, Status, Actions],
            Self::Tables => &[Check, Name, Ref, Project, Created, Edited, Status, Actions],
            Self::Sessions => &[Check, Name, Ref, Project, Agent, Edited, Status, Actions],
            Self::Apps => &[Name, Actions],
        }
    }

    /// `n` of what this tab lists, as a count reads: `1 article`, `3 boards`.
    fn count(self, n: usize, archived: bool) -> String {
        let key = self.kind().key();
        let noun = match archived {
            true => format!("archived {key}"),
            false => key.to_owned(),
        };
        text::count(n, &noun, &format!("{noun}s"))
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
    /// Its heading and width.
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

/// What the library's status menu calls each filter.
trait StatusLabel {
    fn label(self) -> &'static str;
}

impl StatusLabel for Archived {
    fn label(self) -> &'static str {
        match self {
            Self::Exclude => "Active",
            Self::Only => "Archived",
            Self::Include => "All",
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
    status: Archived,
    project: Option<ProjectId>,
    agent: Option<String>,
    labels: Vec<String>,
    order: Order,
    query: String,
}

/// Which of the library's menus is up.
#[derive(Clone, PartialEq, Eq)]
pub(crate) enum LibraryMenu {
    /// A listing's `···`, by the entry it is on.
    Entry(Row),
    /// The tabs, as a select in a narrow bar.
    Tab,
    /// The project picker.
    Project,
    /// The selection's `Move to`.
    Move,
    /// The Status heading.
    Status,
    /// The Agent heading.
    Agent,
}

/// The library, while it stands over the panes: what it lists and how, its
/// selection, and its own menus and label picker. Drawn cached in the detail
/// column's place.
pub(crate) struct Library {
    workspace: Entity<Workspace>,
    root: WeakEntity<Cydonia>,
    menu: ContextMenu<Self, LibraryMenu>,
    labels_picker: Labels,
    tab: Tab,
    status: Archived,
    /// The one project listed, or every open one.
    project: Option<ProjectId>,
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
    /// Every app's program, whose build names its listing.
    apps: Watch,
    _subscriptions: Vec<Subscription>,
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
            project: self.project,
            agent: self.agent.clone(),
            labels: self.labels.clone(),
            order: self.order,
            query: self.query.read(cx).content().to_string(),
        }
    }

    /// A library narrowed as `shelf` says.
    pub(crate) fn shelved(
        shelf: Shelf,
        workspace: Entity<Workspace>,
        root: WeakEntity<Cydonia>,
        cx: &mut Context<Self>,
    ) -> Self {
        let library = Self::new(workspace, root, cx);
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

    pub(crate) fn new(
        workspace: Entity<Workspace>,
        root: WeakEntity<Cydonia>,
        cx: &mut Context<Self>,
    ) -> Self {
        let query = cx.new(|cx| {
            TextField::new(cx)
                .with_frame(false)
                .with_placeholder("Search the library…")
        });
        let subscriptions = vec![
            // What is typed narrows the list: the field's own `notify`
            // repaints the field alone.
            cx.subscribe(&query, |_, _, event: &FieldEvent, cx| {
                if matches!(event, FieldEvent::Changed(_)) {
                    cx.notify();
                }
            }),
            cx.observe(&workspace, |_, _, cx| cx.notify()),
        ];
        Self {
            workspace,
            root,
            menu: ContextMenu::default(),
            labels_picker: Labels::default(),
            tab: Tab::Articles,
            status: Archived::Exclude,
            project: None,
            agent: None,
            labels: Vec::new(),
            order: Order::EDITED,
            query,
            selected: HashSet::new(),
            scroll: UniformListScrollHandle::new(),
            bar: Rc::new(Cell::new(px(0.))),
            strip: Rc::new(Cell::new(px(0.))),
            apps: Watch::default(),
            _subscriptions: subscriptions,
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
    /// An app Cydonia builds from source on this machine.
    local: bool,
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
            false => {
                let (workspace, root) = (self.workspace.clone(), cx.entity().downgrade());
                let library = cx.new(|cx| Library::new(workspace, root, cx));
                self.put_up_library(library, window, cx)
            }
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
        let (workspace, root) = (self.workspace.clone(), cx.entity().downgrade());
        let library = cx.new(|cx| Library::shelved(shelf, workspace, root, cx));
        self.put_up_library(library, window, cx);
    }

    fn put_up_library(
        &mut self,
        library: Entity<Library>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.commit(cx);
        self.library = Some(library);
        // The panes under it are not drawn, and a focus left in one reaches
        // none of the window's commands.
        window.focus(&self.focus, cx);
        cx.notify();
    }
}

impl Library {
    /// Every entry of the tab's kind in every open project, or the one picked,
    /// with what a switch hides left out.
    fn listings(&self, cx: &App) -> Vec<Listing> {
        let library = self;
        let workspace = self.workspace.read(cx);
        let features = &workspace.settings.features;
        if library.tab == Tab::Apps {
            return workspace
                .listed_apps(cx)
                .map(|held| {
                    let program = held.program.read(cx);
                    Listing {
                        row: Row::App(held.id.clone()),
                        title: program.label().into(),
                        reference: None,
                        number: None,
                        project: SharedString::default(),
                        agent: None,
                        touched: 0,
                        created: None,
                        archived: false,
                        labels: None,
                        local: program.local(),
                    }
                })
                .collect();
        }
        let mut out = Vec::new();
        for open in &workspace.projects {
            if library
                .project
                .as_ref()
                .is_some_and(|picked| *picked != open.id)
            {
                continue;
            }
            let project = SharedString::from(open.name());
            let kind = library.tab.kind();
            for (ix, facts) in open.every(kind, cx).enumerate() {
                let row = Row::Entry {
                    project: open.id,
                    showing: facts.showing,
                };
                if !sidebar::shown(&row, features) {
                    continue;
                }
                // A board is referred to by its key, which is what its handles
                // carry; everything else by its number.
                let (reference, number) = match kind {
                    Kind::Board => (
                        Some(open.boards[ix].board.read(cx).key.clone().into()),
                        None,
                    ),
                    _ => (
                        facts.number.map(|n| {
                            SharedString::from(Reference::entry(None, n, None).to_string())
                        }),
                        facts.number,
                    ),
                };
                out.push(Listing {
                    row,
                    title: facts.label.into(),
                    reference,
                    number,
                    project: project.clone(),
                    agent: facts.agent.map(Into::into),
                    touched: facts.touched,
                    created: facts
                        .created
                        .filter(|_| library.tab.fields().contains(&Field::Created)),
                    archived: facts.archived,
                    labels: library.tab.labelled().then_some(facts.labels),
                    local: false,
                });
            }
        }
        out
    }

    /// What the open tab, the query and the sort leave, in order.
    ///
    /// With how many the tab holds before the search and the label filter, to
    /// say how many those hide.
    fn library_listings(&self, cx: &App) -> (Vec<Listing>, usize) {
        let library = self;
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
                !labelled || column::passes(listing.labels.as_deref(), &library.labels)
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
        let library = &mut *self;
        library.tab = tab;
        library.selected.clear();
        // An order by a column the tab does not have would be one nobody
        // can see or undo.
        if !tab.fields().contains(&library.order.field) {
            library.order = Order::EDITED;
        }
        cx.notify();
    }

    fn library_agent(&mut self, agent: Option<String>, cx: &mut Context<Self>) {
        let library = &mut *self;
        library.agent = agent;
        library.selected.clear();
        cx.notify();
    }

    fn library_status(&mut self, status: Archived, cx: &mut Context<Self>) {
        let library = &mut *self;
        library.status = status;
        library.selected.clear();
        cx.notify();
    }

    fn library_project(&mut self, project: Option<ProjectId>, cx: &mut Context<Self>) {
        let library = &mut *self;
        library.project = project;
        library.selected.clear();
        cx.notify();
    }

    fn library_sort_by(&mut self, field: Field, ascending: bool, cx: &mut Context<Self>) {
        let library = &mut *self;
        library.order = Order { field, ascending };
        cx.notify();
    }

    /// A press on `field`'s heading: the sorted column reverses, any other
    /// starts ascending.
    fn library_sort(&mut self, field: Field, cx: &mut Context<Self>) {
        let library = &mut *self;
        library.order = Order {
            field,
            ascending: library.order.field != field || !library.order.ascending,
        };
        cx.notify();
    }

    fn library_select(&mut self, row: Row, cx: &mut Context<Self>) {
        let library = &mut *self;
        if !library.selected.remove(&row) {
            library.selected.insert(row);
        }
        cx.notify();
    }

    /// Take the selection, leaving none.
    fn take_selected(&mut self) -> Vec<Row> {
        self.selected.drain().collect()
    }

    /// The entries in the selection.
    pub(crate) fn selected(&self) -> Vec<Row> {
        self.selected.iter().cloned().collect()
    }

    // ── acting on the selection ──────────────────────────────────

    /// Put every selected entry away, or bring every one back.
    fn archive_selected(&mut self, archived: bool, window: &mut Window, cx: &mut Context<Self>) {
        let rows = self.take_selected();
        self.defer_window(window, cx, move |root, window, cx| {
            for row in &rows {
                root.archive_entry(row, archived, window, cx);
            }
        });
        cx.notify();
    }

    /// Move every selected article into the project at `to`, its folder and
    /// pictures with it. Only articles move between projects.
    fn move_selected(&mut self, to: usize, cx: &mut Context<Self>) {
        let rows = self.take_selected();
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
            if let Some((from, ix)) = row.located(self.workspace.read(cx))
                && from != to
            {
                self.workspace
                    .update(cx, |workspace, cx| workspace.move_article(from, ix, to, cx));
            }
        }
        cx.notify();
    }

    /// Ask before deleting everything selected.
    fn delete_selected(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let doomed = self
            .take_selected()
            .into_iter()
            .map(Doomed::Entry)
            .collect();
        self.defer_window(window, cx, |root, window, cx| {
            root.ask_delete_many(doomed, window, cx)
        });
    }

    /// Open a listing's entry in the drawer over the library. A session before
    /// its first turn has no number to name it by, and opens where it lives,
    /// which puts the library away.
    fn open_listing(&mut self, row: &Row, window: &mut Window, cx: &mut Context<Self>) {
        let (reference, row) = (self.drawn_reference(row, cx), row.clone());
        self.defer_window(window, cx, move |root, window, cx| match reference {
            Some(reference) => root.peek(None, &reference, window, cx),
            None => root.open_row(&row, Opening::Preview, window, cx),
        });
    }

    /// How an entry is named to the drawer, `project#12`. Nothing for a
    /// session before its first turn, which has no number yet.
    fn drawn_reference(&self, row: &Row, cx: &App) -> Option<String> {
        let Row::Entry { project, showing } = row else {
            return None;
        };
        let workspace = self.workspace.read(cx);
        let open = workspace.project(*project)?;
        let number = open.facts(showing, cx)?.number?;
        let name = artifact::reference::project_name(&open.path)?;
        Some(Reference::entry(Some(&name), number, None).to_string())
    }

    // ── chrome ───────────────────────────────────────────────────

    /// The library, in the detail column's place.
    pub(crate) fn library_view(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = Theme::of(cx).clone();
        let library = &*self;
        let selected = !library.selected.is_empty();
        let (listings, total) = self.library_listings(cx);
        let archived = library.status == Archived::Only;
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
            .size_full()
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
        let library = self;
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
        let menu = LibraryMenu::Tab;
        let trigger = theme
            .select_trigger(tab.label())
            .flex_none()
            .id("library-tab")
            .relative()
            .on_click(cx.listener({
                let menu = menu.clone();
                move |this, _, window, cx| {
                    cx.stop_propagation();
                    let shown = shown.clone();
                    let rows = move |_: &mut Self, _: &mut Window, _: &mut Context<Self>| {
                        shown
                            .into_iter()
                            .map(|at| {
                                menu::row::<Self>(
                                    MenuItem::action(at.label()).checked(at == tab),
                                    move |this, _, cx| this.library_tab(at, cx),
                                )
                            })
                            .collect()
                    };
                    this.toggle_menu(menu.clone(), Place::Below, rows, window, cx);
                }
            }));
        let card = self.menu_here(&menu, "library-tab-menu", window, cx);
        self.menu_trigger(trigger, menu, cx).children(card)
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
        let library = self;
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
                    self.library_trigger("library-move", "Move to", LibraryMenu::Move, cx)
                        .children(self.menu_here(
                            &LibraryMenu::Move,
                            "library-move-menu",
                            window,
                            cx,
                        )),
                )
            })
            .when(labelled, |bar| bar.child(self.label_button(cx)))
            .child(
                theme
                    .button("Delete", ButtonStyle::Ghost, None)
                    .id("library-delete")
                    .on_click(cx.listener(|this, _, window, cx| this.delete_selected(window, cx))),
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
        menu: LibraryMenu,
        cx: &Context<Self>,
    ) -> Stateful<Div> {
        let theme = Theme::of(cx);
        let button = theme
            .button(label, ButtonStyle::Ghost, None)
            .id(id)
            .relative()
            .on_click(self.open_library_menu(menu.clone(), cx));
        self.menu_trigger(button, menu, cx)
    }

    /// A click that opens the library's `menu` under what was pressed.
    fn open_library_menu(
        &self,
        menu: LibraryMenu,
        cx: &Context<Self>,
    ) -> impl Fn(&gpui::ClickEvent, &mut Window, &mut App) + 'static {
        cx.listener(move |this, _, window, cx| {
            cx.stop_propagation();
            let key = menu.clone();
            this.toggle_menu(
                menu.clone(),
                Place::Below,
                move |this, _, cx| this.library_rows(&key, cx),
                window,
                cx,
            );
        })
    }

    /// The rows of the library's `menu`.
    fn library_rows(&self, menu: &LibraryMenu, cx: &App) -> Vec<(MenuItem, Act<Self>)> {
        match menu {
            LibraryMenu::Project => self.library_project_rows(cx),
            LibraryMenu::Move => self.move_rows(cx),
            LibraryMenu::Agent => self.agent_rows(cx),
            LibraryMenu::Status => self.status_rows(),
            _ => Vec::new(),
        }
    }

    /// A heading that opens `menu` under itself, its card's id `card`: a
    /// filter on the column, and its sort where it has one.
    #[allow(clippy::too_many_arguments)]
    fn heading_picker(
        &self,
        column: &table::Column,
        sorted: Option<bool>,
        id: &'static str,
        menu: LibraryMenu,
        card: &'static str,
        window: &mut Window,
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
            .on_click(self.open_library_menu(menu.clone(), cx));
        let card = self.menu_here(&menu, card, window, cx);
        self.menu_trigger(cell, menu, cx)
            .children(card)
            .into_any_element()
    }

    /// The menu under a sortable heading's picker: its two sorts, then a line.
    fn sorts(&self, field: Field) -> Vec<(MenuItem, Act<Self>)> {
        let sorted = (self.order.field == field).then_some(self.order.ascending);
        vec![
            menu::row::<Self>(
                MenuItem::action("Sort A → Z")
                    .with_icon(icons::arrows::ArrowDownAZ)
                    .checked(sorted == Some(true)),
                move |this, _, cx| this.library_sort_by(field, true, cx),
            ),
            menu::row::<Self>(
                MenuItem::action("Sort Z → A")
                    .with_icon(icons::arrows::ArrowDownZA)
                    .checked(sorted == Some(false)),
                move |this, _, cx| this.library_sort_by(field, false, cx),
            ),
            menu::row::<Self>(MenuItem::Separator, |_, _, _| {}),
        ]
    }

    fn library_project_rows(&self, cx: &App) -> Vec<(MenuItem, Act<Self>)> {
        let library = self;
        let picked = library.project;
        let mut rows = self.sorts(Field::Project);
        rows.push(menu::row::<Self>(
            MenuItem::action("All projects")
                .with_icon(icons::files::Folders)
                .checked(picked.is_none()),
            |this, _, cx| this.library_project(None, cx),
        ));
        rows.extend(self.workspace.read(cx).projects.iter().map(|open| {
            let project = open.id;
            menu::row::<Self>(
                MenuItem::action(open.name())
                    .with_icon(icons::files::Folder)
                    .checked(picked == Some(project)),
                move |this, _, cx| this.library_project(Some(project), cx),
            )
        }));
        rows
    }

    /// The projects a selection's articles can move into.
    fn move_rows(&self, cx: &App) -> Vec<(MenuItem, Act<Self>)> {
        self.workspace
            .read(cx)
            .projects
            .iter()
            .enumerate()
            .map(|(to, open)| {
                menu::row::<Self>(
                    MenuItem::action(open.name()).with_icon(icons::files::Folder),
                    move |this, _, cx| this.move_selected(to, cx),
                )
            })
            .collect()
    }

    /// The agents a session in an open project runs on, to list one's alone.
    fn agent_rows(&self, cx: &App) -> Vec<(MenuItem, Act<Self>)> {
        let library = self;
        let picked = library.agent.clone();
        let mut names: Vec<String> = self
            .workspace
            .read(cx)
            .chats(cx)
            .map(|chat| chat.entry.name.clone())
            .collect();
        names.sort();
        names.dedup();
        let mut rows = self.sorts(Field::Agent);
        rows.push(menu::row::<Self>(
            MenuItem::action("All agents").checked(picked.is_none()),
            |this, _, cx| this.library_agent(None, cx),
        ));
        rows.extend(names.into_iter().map(|name| {
            let checked = picked.as_ref() == Some(&name);
            menu::row::<Self>(
                MenuItem::action(name.clone()).checked(checked),
                move |this, _, cx| this.library_agent(Some(name.clone()), cx),
            )
        }));
        rows
    }

    fn status_rows(&self) -> Vec<(MenuItem, Act<Self>)> {
        let library = self;
        let picked = library.status;
        Archived::ALL
            .into_iter()
            .map(|status| {
                menu::row::<Self>(
                    MenuItem::action(status.label()).checked(status == picked),
                    move |this, _, cx| this.library_status(status, cx),
                )
            })
            .collect()
    }

    /// The listings as rows under one heading.
    fn library_list(
        &self,
        listings: Rc<Vec<Listing>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = Theme::of(cx).clone();
        let library = self;
        let (order, scroll, tab) = (library.order, library.scroll.clone(), library.tab);
        let fields = tab.fields();
        let columns: Rc<Vec<table::Column>> =
            Rc::new(fields.iter().map(|field| field.column()).collect());
        let headings: Vec<AnyElement> = fields
            .iter()
            .zip(columns.iter())
            .enumerate()
            .map(|(ix, (&field, column))| {
                let sorted = (field.sortable() && order.field == field).then_some(order.ascending);
                match field {
                    Field::Project => {
                        return self.heading_picker(
                            column,
                            sorted,
                            "library-heading-project",
                            LibraryMenu::Project,
                            "library-project-menu",
                            window,
                            cx,
                        );
                    }
                    Field::Agent => {
                        return self.heading_picker(
                            column,
                            sorted,
                            "library-heading-agent",
                            LibraryMenu::Agent,
                            "library-agent-menu",
                            window,
                            cx,
                        );
                    }
                    Field::Status => {
                        return self.heading_picker(
                            column,
                            None,
                            "library-heading-status",
                            LibraryMenu::Status,
                            "library-status-menu",
                            window,
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
        table::table(&theme)
            .flex_1()
            .min_h_0()
            .child(table::header(&theme).flex_none().children(headings))
            .child(rows)
            .into_any_element()
    }

    /// The box that picks a listing out of the selection, or puts it back. On
    /// show while anything is selected; otherwise on the row's hover.
    fn library_check(&self, listing: &Listing, ix: usize, cx: &mut Context<Self>) -> Stateful<Div> {
        let theme = Theme::of(cx).clone();
        let (checked, any) = (
            self.selected.contains(&listing.row),
            !self.selected.is_empty(),
        );
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
        let menu = LibraryMenu::Entry(listing.row.clone());
        let (row, archived) = (listing.row.clone(), listing.archived);
        let open = self.menu.key() == Some(&menu);
        let button = menu::button(
            id,
            icons::layout::Ellipsis,
            menu::Reveal::Group(ROW_GROUP),
            open,
            cx,
        )
        .on_click(cx.listener({
            let menu = menu.clone();
            move |this, _, window, cx| {
                cx.stop_propagation();
                let row = row.clone();
                this.toggle_menu(
                    menu.clone(),
                    Place::BelowEnd,
                    move |this, _, cx| {
                        let rows = this
                            .root
                            .upgrade()
                            .map(|root| root.read(cx).entry_rows(Host::Library, &row, archived, cx))
                            .unwrap_or_default();
                        Self::through(rows)
                    },
                    window,
                    cx,
                );
            }
        }));
        self.menu_trigger(button, menu.clone(), cx)
            .children(self.menu_here(&menu, "library-entry-menu", window, cx))
            .into_any_element()
    }

    fn is_selected(&self, row: &Row) -> bool {
        self.selected.contains(row)
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
                        .text_color(match listing.archived {
                            true => theme.text_faint,
                            false => theme.text,
                        })
                        .child(listing.title.clone()),
                )
                .children(listing.local.then(|| theme.badge("Local")))
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
                        true => Archived::Only,
                        false => Archived::Exclude,
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

impl MenuHost for Library {
    type Key = LibraryMenu;

    fn menu(&self) -> &ContextMenu<Self, LibraryMenu> {
        &self.menu
    }

    fn menu_mut(&mut self) -> &mut ContextMenu<Self, LibraryMenu> {
        &mut self.menu
    }
}

impl InWindow for Library {
    fn root(&self) -> &WeakEntity<Cydonia> {
        &self.root
    }
}

impl LabelHost for Library {
    fn labels(&self) -> &Labels {
        &self.labels_picker
    }

    fn labels_mut(&mut self) -> &mut Labels {
        &mut self.labels_picker
    }

    fn label_workspace(&self) -> &Entity<Workspace> {
        &self.workspace
    }

    fn label_root(&self) -> &WeakEntity<Cydonia> {
        &self.root
    }

    fn selected_rows(&self) -> Vec<Row> {
        self.selected()
    }

    fn label_filter(&mut self) -> Option<&mut Vec<String>> {
        Some(&mut self.labels)
    }
}

impl Render for Library {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let programs: Vec<_> = self.workspace.read(cx).programs().cloned().collect();
        let mut apps = std::mem::take(&mut self.apps);
        apps.set(&programs, cx);
        self.apps = apps;
        self.library_view(window, cx)
    }
}

/// Write the width its parent is laid out at into `into`, and draw again when
/// that moved. The parent must be `relative`.
fn measure(into: Rc<Cell<Pixels>>) -> impl IntoElement {
    gpui::canvas(
        move |bounds, window, _| {
            if into.replace(bounds.size.width) != bounds.size.width {
                window.request_animation_frame();
            }
        },
        |_, _, _, _| {},
    )
    .absolute()
    .size_full()
}
