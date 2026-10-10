//! The library: every entry and card of every open project in one list, with
//! tabs by kind, a search, a sort and a project picker on its headings, and a
//! selection to act on together.
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
use chrono::Datelike as _;
use std::{cell::Cell, collections::HashSet, ops::Range, path::PathBuf, rc::Rc};

mod labels;
pub(crate) use labels::Target;

/// How far back [`Tab::Recent`] reaches, in milliseconds.
const RECENT: u128 = 7 * 24 * 60 * 60 * 1000;

/// The search field's width, and what the bar puts between its parts.
const SEARCH: f32 = 240.;
const BAR_GAP: f32 = 12.;

/// Every list row is drawn at this height: the list lays them out at one
/// extent.
const ROW: f32 = 40.;

/// The hover group a row's checkbox and `···` are revealed by.
const ROW_GROUP: &str = "library-row";

/// The columns, in the order [`Cydonia::library_row`] fills them.
const NAME: usize = 1;
const PROJECT: usize = 3;
const PLACE: usize = 4;
const LABELS: usize = 5;
const EDITED: usize = 6;

/// Which listing the library is on.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Tab {
    All,
    Recent,
    Sessions,
    Articles,
    Boards,
    Cards,
    Tables,
    Archived,
}

impl Tab {
    const ALL: [Self; 8] = [
        Self::All,
        Self::Recent,
        Self::Sessions,
        Self::Articles,
        Self::Boards,
        Self::Cards,
        Self::Tables,
        Self::Archived,
    ];

    fn label(self) -> &'static str {
        match self {
            Self::All => "All",
            Self::Recent => "Recent",
            Self::Sessions => "Sessions",
            Self::Articles => "Articles",
            Self::Boards => "Boards",
            Self::Cards => "Cards",
            Self::Tables => "Tables",
            Self::Archived => "Archived",
        }
    }

    /// The kind a tab lists alone, for the tabs that list one.
    fn kind(self) -> Option<Kind> {
        match self {
            Self::Sessions => Some(Kind::Session),
            Self::Articles => Some(Kind::Article),
            Self::Boards => Some(Kind::Board),
            Self::Cards => Some(Kind::Card),
            Self::Tables => Some(Kind::Table),
            Self::All | Self::Recent | Self::Archived => None,
        }
    }

    /// Whether the kind this tab lists is switched on.
    fn shown(self, features: &Features) -> bool {
        match self.kind() {
            Some(Kind::Session) => features.sessions,
            Some(Kind::Board | Kind::Card) => features.boards,
            Some(Kind::Table) => features.tables,
            Some(Kind::Article) | None => true,
        }
    }

    fn keeps(self, listing: &Listing, now: u128) -> bool {
        match self {
            Self::All => !listing.archived,
            Self::Recent => !listing.archived && now.saturating_sub(listing.touched) < RECENT,
            Self::Archived => listing.archived,
            _ => !listing.archived && self.kind() == Some(listing.kind),
        }
    }
}

/// One line of the library, by what it stands for.
#[derive(Clone, PartialEq, Eq, Hash)]
pub(crate) enum Item {
    Entry(Row),
    /// A card, by its board's project and id, and its own id.
    Card {
        project: PathBuf,
        board: String,
        card: String,
    },
}

/// What the library was narrowed to — what going back to it puts back.
#[derive(Clone, PartialEq)]
pub(crate) struct Shelf {
    tab: Tab,
    project: Option<PathBuf>,
    labels: Vec<String>,
    sort: table::Sort,
    query: String,
}

/// What the library is showing, while it is up.
pub(crate) struct Library {
    tab: Tab,
    /// The one project listed, by its path, or every open one.
    project: Option<PathBuf>,
    /// The labels listed, any one of them; every listing while empty.
    labels: Vec<String>,
    /// The label picker, while one is open.
    picker: Option<labels::Picker>,
    sort: table::Sort,
    query: Entity<TextField>,
    selected: HashSet<Item>,
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
            project: self.project.clone(),
            labels: self.labels.clone(),
            sort: self.sort,
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
            project: shelf.project,
            labels: shelf.labels,
            sort: shelf.sort,
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
            tab: Tab::All,
            project: None,
            labels: Vec::new(),
            picker: None,
            sort: table::Sort {
                column: EDITED,
                ascending: false,
            },
            query,
            selected: HashSet::new(),
            scroll: UniformListScrollHandle::new(),
            bar: Rc::new(Cell::new(px(0.))),
            strip: Rc::new(Cell::new(px(0.))),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Session,
    Board,
    Article,
    Table,
    Card,
}

impl Kind {
    fn icon(self) -> Icon {
        match self {
            Self::Session => icons::social::MessageCircle.into(),
            Self::Board => icons::development::SquareKanban.into(),
            Self::Article => icons::files::FileText.into(),
            Self::Table => icons::files::Table2.into(),
            Self::Card => icons::text::StickyNote.into(),
        }
    }
}

/// One listing as it is drawn, read off the workspace for the frame.
struct Listing {
    item: Item,
    kind: Kind,
    title: SharedString,
    /// `#12` for an entry, the handle for a card.
    reference: Option<SharedString>,
    project: SharedString,
    /// The space an entry is arranged in, or the board and lane a card is on.
    place: Option<SharedString>,
    /// Milliseconds.
    touched: u128,
    archived: bool,
    /// `None` for what carries no labels: a table, a card.
    labels: Option<Vec<String>>,
}

impl Cydonia {
    /// Put the library up, or take it down.
    pub(crate) fn toggle_library(&mut self, cx: &mut Context<Self>) {
        self.commit(cx);
        self.library = match self.library.take() {
            Some(_) => None,
            None => Some(Library::new(cx)),
        };
        cx.notify();
    }

    /// Put the library up narrowed as `shelf` says — where the history goes
    /// back to it.
    pub(crate) fn open_library(&mut self, shelf: Shelf, cx: &mut Context<Self>) {
        self.commit(cx);
        self.library = Some(Library::shelved(shelf, cx));
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
            .on_click(cx.listener(|this, _, _, cx| this.toggle_library(cx)))
            .into_any_element()
    }

    /// Every entry and card of every open project, with what a switch hides
    /// left out.
    fn listings(&self, cx: &App) -> Vec<Listing> {
        let workspace = self.workspace.read(cx);
        let features = &workspace.settings.features;
        let mut out = Vec::new();
        let picked = self
            .library
            .as_ref()
            .and_then(|library| library.project.as_ref());
        for (at, open) in workspace.projects.iter().enumerate() {
            if picked.is_some_and(|picked| *picked != open.path) {
                continue;
            }
            let project = SharedString::from(open.name());
            let row = |showing: Showing| Row::Entry {
                project: open.path.clone(),
                showing,
            };
            let space = |showing: Showing| {
                let member = workspace.member_of(at, showing)?;
                let ix = workspace.space_holding(&member)?;
                Some(SharedString::from(workspace.spaces[ix].label().to_owned()))
            };
            let number = |number: Option<u64>| number.map(|n| SharedString::from(format!("#{n}")));
            let mut entry = |kind,
                             showing: Showing,
                             title: String,
                             n,
                             touched,
                             archived,
                             labels: Option<&[String]>| {
                let row = row(showing.clone());
                if !sidebar::shown(&row, features) {
                    return;
                }
                out.push(Listing {
                    item: Item::Entry(row),
                    kind,
                    title: title.into(),
                    reference: number(n),
                    project: project.clone(),
                    place: space(showing),
                    touched,
                    archived,
                    labels: labels.map(<[String]>::to_vec),
                });
            };
            for chat in &open.sessions {
                entry(
                    Kind::Session,
                    Showing::Session(chat.id),
                    chat.label(),
                    chat.number,
                    chat.touched(),
                    chat.closed,
                    Some(&chat.labels),
                );
            }
            for article in &open.articles {
                entry(
                    Kind::Article,
                    Showing::Article(article.id.clone()),
                    article.label().to_owned(),
                    article.number,
                    article.touched,
                    article.archived,
                    Some(&article.labels),
                );
            }
            for board in &open.boards {
                entry(
                    Kind::Board,
                    Showing::Board(board.id.clone()),
                    board.label().to_owned(),
                    board.number,
                    board.touched,
                    board.archived,
                    Some(&board.labels),
                );
            }
            for table in &open.tables {
                entry(
                    Kind::Table,
                    Showing::Table(table.key.clone()),
                    table.name.clone(),
                    table.number,
                    // The store keeps seconds; every other stamp is milliseconds.
                    table.updated_at.unwrap_or(table.created_at).max(0) as u128 * 1000,
                    table.archived,
                    None,
                );
            }
            if !features.boards {
                continue;
            }
            // An archived board's cards are not held in memory, so only the
            // boards still in hand list theirs.
            for board in open.boards.iter().filter(|board| !board.archived) {
                for column in &board.columns {
                    for card in &column.cards {
                        out.push(Listing {
                            item: Item::Card {
                                project: open.path.clone(),
                                board: board.id.clone(),
                                card: card.id.clone(),
                            },
                            kind: Kind::Card,
                            title: self.card_docs.title(&card.text),
                            reference: board.handle_of(card).map(SharedString::from),
                            project: project.clone(),
                            place: Some(format!("{} › {}", board.label(), column.name).into()),
                            touched: board.touched,
                            archived: false,
                            labels: None,
                        });
                    }
                }
            }
        }
        out
    }

    /// What the open tab, the query and the sort leave, in order.
    fn library_listings(&self, cx: &App) -> Vec<Listing> {
        let Some(library) = &self.library else {
            return Vec::new();
        };
        let now = artifact::stamp::now();
        let mut listings: Vec<Listing> = self
            .listings(cx)
            .into_iter()
            .filter(|listing| library.tab.keeps(listing, now))
            .filter(|listing| labels::passes(listing.labels.as_deref(), &library.labels))
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
        let sort = library.sort;
        let folded =
            |text: &Option<SharedString>| text.as_deref().unwrap_or_default().to_lowercase();
        listings.sort_by(|a, b| {
            let order = match sort.column {
                NAME => a.title.to_lowercase().cmp(&b.title.to_lowercase()),
                PROJECT => a.project.cmp(&b.project),
                PLACE => folded(&a.place).cmp(&folded(&b.place)),
                _ => a.touched.cmp(&b.touched),
            };
            match sort.ascending {
                true => order,
                false => order.reverse(),
            }
        });
        listings
    }

    // ── state ────────────────────────────────────────────────────

    fn library_tab(&mut self, tab: Tab, cx: &mut Context<Self>) {
        if let Some(library) = &mut self.library {
            library.tab = tab;
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

    fn library_sort_by(&mut self, column: usize, ascending: bool, cx: &mut Context<Self>) {
        if let Some(library) = &mut self.library {
            library.sort = table::Sort { column, ascending };
            cx.notify();
        }
    }

    fn library_sort(&mut self, column: usize, cx: &mut Context<Self>) {
        if let Some(library) = &mut self.library {
            library.sort = table::next_sort(Some(library.sort), column);
            cx.notify();
        }
    }

    fn library_select(&mut self, item: Item, cx: &mut Context<Self>) {
        if let Some(library) = &mut self.library {
            if !library.selected.remove(&item) {
                library.selected.insert(item);
            }
            cx.notify();
        }
    }

    /// Take the selection, leaving none.
    fn take_selected(&mut self) -> Vec<Item> {
        self.library
            .as_mut()
            .map(|library| library.selected.drain().collect())
            .unwrap_or_default()
    }

    /// The entries in the selection, cards left out.
    fn selected_rows(&self) -> Vec<Row> {
        self.library
            .iter()
            .flat_map(|library| &library.selected)
            .filter_map(|item| match item {
                Item::Entry(row) => Some(row.clone()),
                Item::Card { .. } => None,
            })
            .collect()
    }

    // ── acting on the selection ──────────────────────────────────

    /// Put every selected entry away, or bring every one back. A card has no
    /// archive of its own and is passed over.
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

    /// Put every selected entry into the space with this id, or into a new one
    /// for `None`. A session that has had no turn has no file for a space to
    /// name, and stays out.
    fn gather_selected(&mut self, space: Option<String>, cx: &mut Context<Self>) {
        let members: Vec<_> = self
            .selected_rows()
            .iter()
            .filter_map(|row| self.member_of_row(row, cx))
            .collect();
        self.take_selected();
        self.workspace.update(cx, |workspace, cx| {
            workspace.gather(space.as_deref(), &members, cx)
        });
        cx.notify();
    }

    /// Ask before deleting everything selected.
    fn delete_selected(&mut self, cx: &mut Context<Self>) {
        let doomed = self
            .take_selected()
            .into_iter()
            .map(|item| match item {
                Item::Entry(row) => Doomed::Entry(row),
                Item::Card { board, card, .. } => Doomed::Card(board, card),
            })
            .collect();
        self.ask_delete_many(doomed, cx);
    }

    /// Open what a listing stands for in the drawer over the library. A session before its first turn has no
    /// number to name it by, and opens where it lives, which puts the library
    /// away.
    fn open_item(&mut self, item: &Item, window: &mut Window, cx: &mut Context<Self>) {
        match item {
            Item::Entry(row) => match self.drawn_reference(row, cx) {
                Some(reference) => self.peek(None, &reference, window, cx),
                None => self.open_row(row, window, cx),
            },
            Item::Card {
                project,
                board,
                card,
            } => {
                let member = {
                    let workspace = self.workspace.read(cx);
                    workspace.project_at(project).and_then(|at| {
                        let ix = workspace.projects[at].board_ix(board)?;
                        workspace.board_member(at, ix)
                    })
                };
                if let Some(member) = member {
                    self.open_card(None, member, card.clone(), window, cx);
                }
            }
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
        let listings = Rc::new(self.library_listings(cx));
        let bar = match selected {
            true => self.library_selection_bar(window, cx),
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
    fn library_selection_bar(&self, window: &mut Window, cx: &mut Context<Self>) -> Div {
        let theme = Theme::of(cx).clone();
        let Some(library) = &self.library else {
            return div();
        };
        let count = library.selected.len();
        let rows = self.selected_rows();
        let archived = library.tab == Tab::Archived;
        let articles = rows.iter().any(|row| {
            matches!(
                row,
                Row::Entry {
                    showing: Showing::Article(_),
                    ..
                }
            )
        });
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
            .when(!rows.is_empty(), |bar| {
                bar.child(
                    theme
                        .button(
                            match archived {
                                true => "Unarchive",
                                false => "Archive",
                            },
                            ButtonStyle::Ghost,
                            None,
                        )
                        .id("library-archive")
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.archive_selected(!archived, window, cx)
                        })),
                )
            })
            .when(articles && projects > 1, |bar| {
                bar.child(
                    self.library_trigger("library-move", "Move to", Menu::LibraryMove, cx)
                        .children(self.move_menu(window, cx)),
                )
            })
            .when(!rows.is_empty(), |bar| {
                bar.child(
                    self.library_trigger("library-space", "Add to space", Menu::LibrarySpace, cx)
                        .children(self.space_menu(window, cx)),
                )
            })
            .when(self.selection_labelled(cx), |bar| {
                bar.child(self.label_button(cx))
            })
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

    /// The Project heading: the one project listed, or every open one, and
    /// the menu that picks it and sorts by it.
    fn project_heading(
        &self,
        column: &table::Column,
        sorted: Option<bool>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = Theme::of(cx).clone();
        let menu = Menu::LibraryProject;
        let cell = table::header_cell(&theme, column, sorted)
            .child(
                icons::icon(icons::arrows::ChevronDown)
                    .size(px(11.))
                    .text_color(theme.text_muted),
            )
            .id(("library-heading", PROJECT))
            .relative()
            .on_click(cx.listener({
                let menu = menu.clone();
                move |this, _, _, cx| {
                    cx.stop_propagation();
                    this.toggle_menu(menu.clone(), cx);
                }
            }));
        self.menu_press(cell, menu, cx)
            .children(self.library_project_menu(window, cx))
            .into_any_element()
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
        let sorted = (library.sort.column == PROJECT).then_some(library.sort.ascending);
        let mut rows = vec![
            menu::row(
                MenuItem::action("Sort A → Z")
                    .with_icon(icons::arrows::ArrowDownAZ)
                    .checked(sorted == Some(true)),
                |this, _, cx| this.library_sort_by(PROJECT, true, cx),
            ),
            menu::row(
                MenuItem::action("Sort Z → A")
                    .with_icon(icons::arrows::ArrowDownZA)
                    .checked(sorted == Some(false)),
                |this, _, cx| this.library_sort_by(PROJECT, false, cx),
            ),
            menu::row(MenuItem::Separator, |_, _, _| {}),
            menu::row(
                MenuItem::action("All projects")
                    .with_icon(icons::files::Folders)
                    .checked(picked.is_none()),
                |this, _, cx| this.library_project(None, cx),
            ),
        ];
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

    /// The spaces a selection can join, and a new one.
    fn space_menu(&self, window: &mut Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        if self.menu != Some(Menu::LibrarySpace) {
            return None;
        }
        let mut rows = vec![menu::row(
            MenuItem::action("New space").with_icon(icons::math::Plus),
            |this, _, cx| this.gather_selected(None, cx),
        )];
        rows.extend(self.workspace.read(cx).spaces.iter().map(|space| {
            let id = space.id.clone();
            menu::row(
                MenuItem::action(space.label().to_owned())
                    .with_icon(icons::layout::LayoutDashboard),
                move |this, _, cx| this.gather_selected(Some(id.clone()), cx),
            )
        }));
        let id = SharedString::from("library-space-menu");
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
        let (sort, scroll) = (library.sort, library.scroll.clone());
        let columns = Rc::new(columns());
        let width = least(&columns);
        let headings: Vec<AnyElement> = columns
            .iter()
            .enumerate()
            .map(|(ix, column)| {
                let sortable = matches!(ix, NAME | PROJECT | PLACE | EDITED);
                let sorted = (sortable && sort.column == ix).then_some(sort.ascending);
                if ix == PROJECT {
                    return self.project_heading(column, sorted, window, cx);
                }
                if ix == LABELS {
                    return self.labels_heading(column, cx);
                }
                let cell = table::header_cell(&theme, column, sorted).id(("library-heading", ix));
                match sortable {
                    true => cell
                        .on_click(cx.listener(move |this, _, _, cx| this.library_sort(ix, cx)))
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
                    .map(|ix| this.library_row(&listings[ix], ix, &columns, window, cx))
                    .collect::<Vec<_>>()
            }),
        )
        .track_scroll(&scroll)
        // TODO(crabtalk/zed#11): drop once bezel-gpui no longer remaps a wheel onto the
        // other axis by default. `UniformList` has no builder for it.
        .map(|mut list| {
            list.interactivity().base_style.restrict_scroll_to_axis = Some(true);
            list
        })
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
                library.selected.contains(&listing.item),
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
                let item = listing.item.clone();
                move |this, _, _, cx| {
                    cx.stop_propagation();
                    this.library_select(item.clone(), cx);
                }
            }))
    }

    /// A listing's `···`: the sidebar's menu for an entry, the board's for a
    /// card.
    fn library_actions(
        &self,
        listing: &Listing,
        ix: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let id = SharedString::from(format!("library-menu-{ix}"));
        match &listing.item {
            Item::Entry(row) => {
                let menu = Menu::Library(row.clone());
                self.menu_button(
                    id,
                    Some(ROW_GROUP),
                    icons::layout::Ellipsis,
                    menu.clone(),
                    cx,
                )
                .children(self.entry_menu(menu, row, listing.archived, window, cx))
                .into_any_element()
            }
            Item::Card { board, card, .. } => self
                .menu_button(
                    id,
                    Some(ROW_GROUP),
                    icons::layout::Ellipsis,
                    Menu::Card(card.clone()),
                    cx,
                )
                .children(self.card_menu(board, card, window, cx))
                .into_any_element(),
        }
    }

    fn is_selected(&self, item: &Item) -> bool {
        self.library
            .as_ref()
            .is_some_and(|library| library.selected.contains(item))
    }

    /// One line of the list.
    fn library_row(
        &self,
        listing: &Listing,
        ix: usize,
        columns: &[table::Column],
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = Theme::of(cx).clone();
        let checked = self.is_selected(&listing.item);
        let muted = |text: Option<SharedString>| {
            div()
                .min_w_0()
                .truncate()
                .text_color(theme.text_muted)
                .child(text.unwrap_or_default())
                .into_any_element()
        };
        let name = div()
            .min_w_0()
            .flex()
            .flex_row()
            .items_center()
            .gap(px(8.))
            .child(
                icons::icon(listing.kind.icon())
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
            .into_any_element();
        let reference = div()
            .text_style(TextStyle::Caption)
            .text_color(theme.text_faint)
            .child(listing.reference.clone().unwrap_or_default())
            .into_any_element();
        let cells = vec![
            self.library_check(listing, ix, cx).into_any_element(),
            name,
            reference,
            muted(Some(listing.project.clone())),
            muted(listing.place.clone()),
            self.label_cell(&listing.item, listing.labels.as_deref(), ix, cx),
            muted(Some(edited(listing.touched).into())),
            self.library_actions(listing, ix, window, cx),
        ];
        table::row(&theme, columns, ix == 0, checked, cells)
            .h(px(ROW))
            .id(("library-row", ix))
            .group(ROW_GROUP)
            .cursor_pointer()
            .on_click(cx.listener({
                let item = listing.item.clone();
                move |this, _, window, cx| this.open_item(&item, window, cx)
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

/// The columns, declared once for the heading and every row. A flexible
/// column's weight is the least width it reads at, so at the table's least
/// width each is exactly that.
fn columns() -> Vec<table::Column> {
    vec![
        table::Column::new("", table::Width::Fixed(px(40.))),
        table::Column::new("Name", table::Width::Flex(240.)),
        table::Column::new("Ref", table::Width::Fixed(px(88.))),
        table::Column::new("Project", table::Width::Flex(140.)),
        table::Column::new("Location", table::Width::Flex(180.)),
        table::Column::new("Labels", table::Width::Flex(160.)),
        table::Column::new("Edited", table::Width::Fixed(px(120.))),
        table::Column::new("", table::Width::Fixed(px(44.))),
    ]
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

/// When something was last written, as coarse as still says something: the
/// time today, the day this year, the date before that.
fn edited(touched: u128) -> String {
    let Some(at) = i64::try_from(touched)
        .ok()
        .filter(|ms| *ms > 0)
        .and_then(chrono::DateTime::from_timestamp_millis)
    else {
        return String::new();
    };
    let at = at.with_timezone(&chrono::Local);
    let now = chrono::Local::now();
    let format = match () {
        _ if at.date_naive() == now.date_naive() => "%-I:%M %p",
        _ if at.year() == now.year() => "%b %-d",
        _ => "%b %-d, %Y",
    };
    at.format(format).to_string()
}
