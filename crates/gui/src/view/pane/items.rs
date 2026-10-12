//! The views a pane holds: one per board, article, table and session it has
//! shown, made the first time it shows each and kept so that showing one again
//! finds it where the pane left it.

use super::{Tab, find::Searchable as _};
use crate::view::{
    article::ArticleView,
    board::BoardView,
    component::transcript::{Drawn, SessionView},
    table::TableView,
};
use artifact::{kind::Kind, space::Member};
use bezel::gpui::{App, AppContext as _, Context, Entity};

impl Tab {
    /// The entry of `kind` the pane shows: its own, or for the pane a window
    /// with no space open shows, what the project in front is pointed at.
    fn shown_of(&self, kind: Kind, cx: &App) -> Option<Member> {
        if let Some(entry) = &self.entry {
            return (entry.kind == kind).then(|| entry.clone());
        }
        let workspace = self.workspace.read(cx);
        let project = workspace.active?;
        let open = workspace.projects.get(project)?;
        match kind {
            Kind::Board => workspace.board_member(project, open.board?),
            Kind::Article => workspace.article_member(project, open.article?),
            Kind::Table => workspace.table_member(project, open.table?),
            Kind::Session => None,
        }
    }

    /// The session the pane shows.
    pub(crate) fn shown_session(&self, cx: &App) -> Option<u64> {
        self.workspace
            .read(cx)
            .session_id_of(self.entry.as_ref(), cx)
    }

    /// Make the views for what the pane is about to draw.
    pub(crate) fn sync_items(&mut self, cx: &mut Context<Self>) {
        if let Some(board) = self.shown_of(Kind::Board, cx) {
            self.board_view(board, cx);
        }
        if let Some(article) = self.shown_of(Kind::Article, cx) {
            let view = self.article_view(article, cx);
            view.update(cx, |view, cx| view.ensure_editor(cx));
        }
        if let Some(table) = self.shown_of(Kind::Table, cx) {
            self.table_view(table, cx);
        }
        if let Some(session) = self.shown_session(cx) {
            self.session_view(session, cx);
        }
    }

    /// The pane's view of `board`, made the first time it shows it.
    pub(crate) fn board_view(
        &mut self,
        board: Member,
        cx: &mut Context<Self>,
    ) -> Entity<BoardView> {
        if let Some(view) = self.boards.iter().find(|view| view.read(cx).board == board) {
            return view.clone();
        }
        let (workspace, root, pane) = (
            self.workspace.clone(),
            self.root.clone(),
            self.entry.clone(),
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
        let board = self.shown_of(Kind::Board, cx)?;
        self.boards
            .iter()
            .find(|view| view.read(cx).board == board)
            .cloned()
    }

    /// The pane's view of `article`, made the first time it shows it.
    pub(crate) fn article_view(
        &mut self,
        article: Member,
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
        let article = self.shown_of(Kind::Article, cx)?;
        self.articles
            .iter()
            .find(|view| view.read(cx).article == article)
            .cloned()
    }

    /// The pane's view of `table`, made the first time it shows it.
    pub(crate) fn table_view(
        &mut self,
        table: Member,
        cx: &mut Context<Self>,
    ) -> Entity<TableView> {
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
        let table = self.shown_of(Kind::Table, cx)?;
        self.tables
            .iter()
            .find(|view| view.read(cx).table == table)
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
            self.entry.clone(),
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
