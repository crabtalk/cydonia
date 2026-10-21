//! One tab of a pane: what it shows, and what it holds while it shows it —
//! its items, its find bar, its drawer, its formatting bar and its composer.

use crate::{
    model::workspace::{Showing, Workspace},
    view::{
        app, article,
        board::{self, DismissFind, info::BoardInfo},
        component::{composer::Composer, ribbon::Ribbon, transcript},
        kit::{
            rename::{NameEvent, NameField},
            watch::Watch,
        },
        pane::Edge,
        sidebar::Renaming,
        table,
        window::{reach::InWindow, root::Cydonia},
    },
};
use artifact::kind::Kind;
use bezel::{
    gpui::{
        AnyElement, App, Context, Entity, FocusHandle, Render, WeakEntity, Window, div, prelude::*,
    },
    theme::Theme,
    ui::{
        context_menu::ContextMenu,
        icons,
        input::{FieldEvent, TextField},
        widgets::Content as _,
    },
};
use std::rc::Rc;

/// Which of a pane's menus is up.
#[derive(Clone, PartialEq, Eq)]
pub(crate) enum TabMenu {
    /// The block picker on the article ribbon — see
    /// [`crate::view::component::ribbon`].
    Turn,
    /// The `···` on the title row: the entry's own menu — see
    /// [`crate::view::pane::title`].
    Entry,
    /// The title row's move, where the pane can go more than one way.
    Move,
    /// The title row's deck: the entries the pane's stack holds.
    Stack,
}

/// One entry of a pane's stack: what it shows, and what it holds while it
/// shows it.
pub(crate) struct Tab {
    /// The entry this tab is on, named the way a space names its members —
    /// which project it is in and which of that project's things it is.
    pub(crate) entry: artifact::space::Member,
    pub(crate) workspace: Entity<Workspace>,
    pub(crate) root: WeakEntity<Cydonia>,
    /// The board views' shared card previews and drag domain — see
    /// [`Cydonia::card_docs`] and [`Cydonia::board_sort`].
    pub(crate) card_docs: Rc<board::Docs>,
    pub(crate) board_sort: bezel::ui::drag::Domain<board::BoardRegion, board::BoardItem>,
    /// Tracked on the pane this leaf draws, so the pane is an ancestor of the
    /// focused element and the chords claimed on it are reached — an action
    /// runs only through the focused element's ancestors. Where the focus lands
    /// for a pane holding nothing to type into; a session's composer and a
    /// document's editor are inside the pane already.
    pub(crate) focus: FocusHandle,
    /// Whether this is the pane with the window's focus — see
    /// [`Cydonia::sync_focus`].
    pub(crate) focused: bool,
    /// Where its pane stands against the window's top edge — see
    /// [`Cydonia::sync_edges`].
    pub(crate) edge: Edge,
    pub(crate) composer: Entity<Composer>,
    pub(crate) queued_galleries: std::collections::HashMap<
        (u64, usize, String),
        Entity<crate::view::component::transcript::gallery::Gallery>,
    >,
    /// What the pane's drawer holds, while it is up — see
    /// [`crate::view::pane::drawer`].
    pub(crate) drawer: Option<super::drawer::Drawer>,
    /// The pane's view of each board it has shown, so a board shown again
    /// is where this pane left it.
    pub(crate) boards: Vec<Entity<board::BoardView>>,
    /// The same for each article.
    pub(crate) articles: Vec<Entity<article::ArticleView>>,
    /// The same for each session.
    pub(crate) sessions: Vec<Entity<transcript::SessionView>>,
    /// The same for each table.
    pub(crate) tables: Vec<Entity<table::TableView>>,
    /// The same for each app.
    pub(crate) apps: Vec<Entity<app::AppView>>,
    /// The board's find field, and whether its bar is up.
    ///
    /// The bar stands whenever the query does: a board narrowed with nothing on
    /// screen saying so is a board quietly missing cards, and a drag that lands
    /// in a lane it cannot see is worse. Closing it is what clears the query —
    /// see [`crate::view::window::root::Cydonia::dismiss_find`].
    pub(crate) find_field: Entity<TextField>,
    pub(crate) finding: bool,
    /// Which find hit an article or a transcript is on, counted from the
    /// first — see [`crate::view::pane::find`].
    pub(crate) find_at: usize,
    /// Whether an article's or a transcript's find reads its field as a
    /// regular expression.
    pub(crate) find_pattern: bool,
    /// The workspace search applied, which the find stands for while the bar
    /// is down — see [`Cydonia::push_finds`].
    pub(crate) applied: Option<artifact::search::Query>,
    /// The formatting bar over the open document's selection, and the URL
    /// field it puts up — see [`crate::view::component::ribbon`].
    pub(crate) ribbon: Ribbon,
    pub(crate) menu: ContextMenu<Self, TabMenu>,
    /// What the title row's name field is attached to, and the field itself.
    pub(crate) renaming: Option<Renaming>,
    pub(crate) name_field: Entity<NameField>,
    /// The board identity panel, while it is open — see
    /// [`crate::view::board::info::BoardInfo`].
    pub(crate) info: Option<BoardInfo>,
    /// Whether the press now being handled landed on the name of the board
    /// whose panel is open — read by [`Tab::toggle_info`] and nothing else.
    pub(crate) info_pressed: bool,
    /// The sessions it draws — see [`Self::watch_sessions`].
    sessions_watch: Watch,
}

impl Tab {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        entry: artifact::space::Member,
        workspace: Entity<Workspace>,
        root: WeakEntity<Cydonia>,
        card_docs: Rc<board::Docs>,
        board_sort: bezel::ui::drag::Domain<board::BoardRegion, board::BoardItem>,
        composer: Entity<Composer>,
        cx: &mut Context<Self>,
    ) -> Self {
        let find_field = board::find_field(cx);
        // What the pane draws is read off the model, and a cached pane draws
        // again only when told.
        cx.observe(&workspace, |_, _, cx| cx.notify()).detach();
        cx.subscribe(&find_field, |this, _, event: &FieldEvent, cx| {
            if let FieldEvent::Changed(_) = event {
                this.find_changed(cx);
            }
        })
        .detach();
        let name_field = cx.new(NameField::new);
        cx.subscribe(&name_field, |this, _, event: &NameEvent, cx| match event {
            NameEvent::Named(name) => this.commit_name(name.clone(), cx),
            NameEvent::Dismissed => this.dismiss_name(cx),
        })
        .detach();
        Self {
            entry,
            workspace,
            root,
            card_docs,
            board_sort,
            focus: cx.focus_handle(),
            focused: false,
            edge: Edge::default(),
            composer,
            queued_galleries: Default::default(),
            drawer: None,
            boards: Vec::new(),
            articles: Vec::new(),
            sessions: Vec::new(),
            tables: Vec::new(),
            apps: Vec::new(),
            find_field,
            finding: false,
            find_at: 0,
            find_pattern: false,
            applied: None,
            ribbon: Ribbon::new(cx),
            menu: ContextMenu::default(),
            renaming: None,
            name_field,
            info: None,
            info_pressed: false,
            sessions_watch: Watch::default(),
        }
    }
}

impl Tab {
    /// What the tab shows: its entry, where its project is open and it is
    /// still there.
    pub(crate) fn showing(&self, cx: &App) -> Option<(usize, Showing)> {
        self.workspace.read(cx).showing_of(&self.entry, cx)
    }

    /// Observe the sessions the pane draws: the one it shows, and the
    /// thread under the article it shows.
    fn watch_sessions(&mut self, cx: &mut Context<Self>) {
        let workspace = self.workspace.read(cx);
        let thread = self
            .thread_article(cx)
            .and_then(|path| workspace.article_thread(&path, cx));
        let sessions: Vec<_> = [self.shown_session(cx), thread]
            .into_iter()
            .flatten()
            .filter_map(|id| workspace.session_entity(id).cloned())
            .collect();
        let mut watch = std::mem::take(&mut self.sessions_watch);
        watch.set(&sessions, cx);
        self.sessions_watch = watch;
    }

    /// What the pane shows, drawn: the item, and what the pane hangs over it.
    fn body(
        &mut self,
        showing: &Showing,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        match showing {
            Showing::Session(id) => self.conversation(Some(*id), cx),
            Showing::Board(_) => {
                let Some(view) = self.board_shown(cx) else {
                    return div().flex_1().into_any_element();
                };
                let pill = match cx.has_active_drag() {
                    true => None,
                    false => self.search_pill(cx),
                };
                div()
                    .flex_1()
                    .min_h_0()
                    .relative()
                    .flex()
                    .flex_col()
                    .on_action(cx.listener(|this, _: &DismissFind, _, cx| this.dismiss_find(cx)))
                    .child(view)
                    .children(pill)
                    .into_any_element()
            }
            Showing::Article(_) => {
                let Some(view) = self
                    .article_shown(cx)
                    .filter(|view| view.read(cx).editor().is_some())
                else {
                    return div().flex_1().into_any_element();
                };
                let focused = self.focused;
                div()
                    .flex_1()
                    .min_h_0()
                    .w_full()
                    .relative()
                    .flex()
                    .flex_col()
                    .child(view)
                    .children(self.search_pill(cx))
                    // Last, and floated over the document from where the
                    // selection ends — the bar is chrome the page runs under.
                    .children(self.ribbon_bar(focused, window, cx))
                    .into_any_element()
            }
            Showing::Table(_) => match self.table_shown(cx) {
                Some(view) => view.into_any_element(),
                None => div().flex_1().into_any_element(),
            },
        }
    }

    /// An app the pane shows, drawn.
    fn app_body(&self, cx: &App) -> AnyElement {
        match self.app_shown(cx) {
            Some(view) => div()
                .flex_1()
                .min_h_0()
                .flex()
                .flex_col()
                .child(view)
                .into_any_element(),
            None => div().flex_1().into_any_element(),
        }
    }

    /// What the pane's drawer draws for what it holds.
    fn drawer_face(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<super::drawer::Face> {
        let drawer = self.drawer_shown(cx)?;
        match &drawer.peek {
            super::drawer::Peek::Page(view) => view
                .clone()
                .update(cx, |view, cx| view.page_face(window, cx)),
            super::drawer::Peek::Entry(entry) => {
                let (entry, scroll, on) =
                    (entry.clone(), drawer.scroll.clone(), self.entry.clone());
                self.root
                    .update(cx, |root, cx| {
                        root.entry_face(&entry, Some(&on), &scroll, window, cx)
                    })
                    .ok()
            }
        }
    }
}

impl InWindow for Tab {
    fn root(&self) -> &WeakEntity<Cydonia> {
        &self.root
    }
}

impl Render for Tab {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.sync_items(cx);
        self.watch_sessions(cx);
        let theme = Theme::of(cx).clone();
        let showing = self.showing(cx);
        let app = self.entry.kind() == Kind::App;
        let body = match &showing {
            _ if app => self.app_body(cx),
            // The entry has gone since the tab named it. The tab says so
            // rather than standing empty, and is about to be dropped — see
            // [`crate::model::workspace::Workspace::prune_spaces`].
            None => theme
                .empty_state(
                    icons::files::File,
                    "This entry has gone",
                    "It was deleted after the tab was opened.",
                )
                .into_any_element(),
            Some((_, showing)) => self.body(showing, window, cx),
        };
        let session = match &showing {
            Some((_, Showing::Session(id))) => Some(*id),
            _ => None,
        };
        let footer = match (&showing, session) {
            (_, Some(id)) => Some(self.session_footer(Some(id), cx)),
            (Some((_, Showing::Article(_))), None) => self.thread_footer(cx),
            _ => None,
        };
        let selection = match session.is_some() && self.focused {
            true => self.selection_bar(cx),
            false => None,
        };
        let drawer = match self.drawer_face(window, cx) {
            Some(face) => self.drawer_layer(face, cx),
            None => Vec::new(),
        };
        let composer = self.composer.clone();
        let title = self.title_row(window, cx);
        div()
            .relative()
            .flex_1()
            .min_h_0()
            .size_full()
            .flex()
            .flex_col()
            .on_action(cx.listener(Self::commit_info))
            .on_action(cx.listener(Self::dismiss_info))
            .children(title)
            .child(body)
            .children(footer)
            .children(selection)
            // The whole pane takes a dropped picture for the composer.
            .when(session.is_some(), |pane| {
                pane.on_drop(move |paths: &bezel::gpui::ExternalPaths, _, cx| {
                    composer.update(cx, |composer, cx| composer.drop_paths(paths, cx));
                })
                .child(crate::view::detail::drop_wash(&theme))
            })
            .children(drawer)
    }
}
