//! The views a pane holds: one per board, article, table, app and session it has
//! shown, made the first time it shows each and kept so that showing one again
//! finds it where the pane left it.

use super::{Tab, find::Searchable as _};
use crate::{
    model::workspace::{Entry, Member},
    view::{
        app::AppView,
        article::ArticleView,
        board::BoardView,
        component::transcript::{Drawn, SessionView},
        table::TableView,
    },
};
use artifact::kind::Kind;
use bezel::gpui::{App, AppContext as _, Context, Entity};

impl Tab {
    /// The entry of `kind` the tab shows.
    fn shown_of(&self, kind: Kind) -> Option<Entry> {
        self.entry
            .entry()
            .filter(|entry| entry.kind == kind)
            .cloned()
    }

    /// The app the tab shows.
    fn shown_app(&self) -> Option<String> {
        match &self.entry {
            Member::App { app } => Some(app.clone()),
            Member::Entry(_) => None,
        }
    }

    /// The session the pane shows.
    pub(crate) fn shown_session(&self, cx: &App) -> Option<u64> {
        self.workspace.read(cx).session_id_of(Some(&self.entry), cx)
    }

    /// Make the views for what the pane is about to draw.
    pub(crate) fn sync_items(&mut self, cx: &mut Context<Self>) {
        if let Some(board) = self.shown_of(Kind::Board) {
            self.board_view(board, cx);
        }
        if let Some(article) = self.shown_of(Kind::Article) {
            let view = self.article_view(article, cx);
            view.update(cx, |view, cx| view.ensure_editor(cx));
        }
        if let Some(table) = self.shown_of(Kind::Table) {
            self.table_view(table, cx);
        }
        if let Some(app) = self.shown_app() {
            self.app_view(app, cx);
        }
        if let Some(session) = self.shown_session(cx) {
            self.session_view(session, cx);
        }
    }

    /// The pane's view of `board`, made the first time it shows it.
    pub(crate) fn board_view(&mut self, board: Entry, cx: &mut Context<Self>) -> Entity<BoardView> {
        if let Some(view) = self.boards.iter().find(|view| view.read(cx).board == board) {
            return view.clone();
        }
        let (workspace, root, pane) = (
            self.workspace.clone(),
            self.root.clone(),
            Some(self.entry.clone()),
        );
        let (docs, sort, find) = (
            self.card_docs.clone(),
            self.board_sort.clone(),
            self.find(cx),
        );
        let view = cx.new(|cx| BoardView::new(board, pane, workspace, root, docs, sort, cx));
        view.update(cx, |view, cx| view.set_find(find, cx));
        self.boards.push(view.clone());
        view
    }

    /// The board view the pane draws, where it shows a board.
    pub(crate) fn board_shown(&self, cx: &App) -> Option<Entity<BoardView>> {
        let board = self.shown_of(Kind::Board)?;
        self.boards
            .iter()
            .find(|view| view.read(cx).board == board)
            .cloned()
    }

    /// The pane's view of `article`, made the first time it shows it.
    pub(crate) fn article_view(
        &mut self,
        article: Entry,
        cx: &mut Context<Self>,
    ) -> Entity<ArticleView> {
        if let Some(view) = self
            .articles
            .iter()
            .find(|view| view.read(cx).article == article)
        {
            return view.clone();
        }
        let (workspace, root, find) = (self.workspace.clone(), self.root.clone(), self.find(cx));
        let view = cx.new(|cx| ArticleView::new(article, workspace, root, cx));
        view.update(cx, |view, cx| view.set_find(find, cx));
        self.articles.push(view.clone());
        view
    }

    /// The article view the pane draws, where it shows an article.
    pub(crate) fn article_shown(&self, cx: &App) -> Option<Entity<ArticleView>> {
        let article = self.shown_of(Kind::Article)?;
        self.articles
            .iter()
            .find(|view| view.read(cx).article == article)
            .cloned()
    }

    /// The pane's view of `table`, made the first time it shows it.
    pub(crate) fn table_view(&mut self, table: Entry, cx: &mut Context<Self>) -> Entity<TableView> {
        if let Some(view) = self.tables.iter().find(|view| view.read(cx).table == table) {
            return view.clone();
        }
        let (workspace, root) = (self.workspace.clone(), self.root.clone());
        let view = cx.new(|cx| TableView::new(table, workspace, root, cx));
        self.tables.push(view.clone());
        view
    }

    /// The table view the pane draws, where it shows a table.
    pub(crate) fn table_shown(&self, cx: &App) -> Option<Entity<TableView>> {
        let table = self.shown_of(Kind::Table)?;
        self.tables
            .iter()
            .find(|view| view.read(cx).table == table)
            .cloned()
    }

    /// The pane's view of the app `app`, made the first time it shows it.
    pub(crate) fn app_view(&mut self, app: String, cx: &mut Context<Self>) -> Entity<AppView> {
        if let Some(view) = self.apps.iter().find(|view| view.read(cx).app == app) {
            return view.clone();
        }
        let workspace = self.workspace.clone();
        let view = cx.new(|cx| AppView::new(app, workspace, cx));
        self.apps.push(view.clone());
        view
    }

    /// The app view the pane draws, where it shows an app.
    pub(crate) fn app_shown(&self, cx: &App) -> Option<Entity<AppView>> {
        let app = self.shown_app()?;
        self.apps
            .iter()
            .find(|view| view.read(cx).app == app)
            .cloned()
    }

    /// The pane's view of session `id`, made the first time it shows it.
    pub(crate) fn session_view(&mut self, id: u64, cx: &mut Context<Self>) -> Entity<SessionView> {
        if let Some(view) = self
            .sessions
            .iter()
            .find(|view| view.read(cx).session == id)
        {
            return view.clone();
        }
        let (workspace, root, pane) = (
            self.workspace.clone(),
            self.root.clone(),
            Some(self.entry.clone()),
        );
        let find = self.find(cx);
        let view = cx.new(|cx| SessionView::new(id, pane, Drawn::Pane, 0., workspace, root, cx));
        view.update(cx, |view, cx| view.set_find(find, cx));
        self.sessions.push(view.clone());
        view
    }

    /// The session view the pane draws, where it shows a session.
    pub(crate) fn session_shown(&self, cx: &App) -> Option<Entity<SessionView>> {
        let id = self.shown_session(cx)?;
        self.sessions
            .iter()
            .find(|view| view.read(cx).session == id)
            .cloned()
    }
}
