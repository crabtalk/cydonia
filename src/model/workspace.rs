//! What the app *is*, as opposed to what it draws: the projects that are
//! open, the sessions running in them, and the appearance the user picked.
//!
//! Views hold an `Entity<Workspace>` and read it; they never own a copy of any
//! of this. A mutation here notifies, and whichever views are observing repaint
//! — so nothing has to remember to tell the chrome that a session appeared.
//!
//! Everything [`crate::model::state`] persists lives here and nowhere else, which is
//! why [`Workspace::save`] can take no arguments.

use crate::{
    agent,
    data::{ColType, Column, Data, Edit, Page, Table},
    model::{
        article::{self, Article},
        board::{self, Board},
        project::Project,
        record,
        session::ChatSession,
        settings::{self, Settings},
        state::{self, State},
    },
};
use bezel::{
    gpui::{App, Context, EntityId, SharedString},
    theme::{self, Brand, Theme, Tint, appearance::AppearanceMode},
    ui::input,
};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

/// What a table is called before it is named.
const UNTITLED: &str = "Untitled";

/// And a column.
const COLUMN: &str = "Column";

pub struct Workspace {
    pub settings: Settings,
    pub projects: Vec<Project>,
    pub active: Option<usize>,
    pub appearance: AppearanceMode,
    pub reduce_transparency: bool,
    pub cursor_blink: bool,
    /// Session ids are minted here and never reused, so a card's link to the
    /// session it opened stays unambiguous for the life of the process.
    next_id: u64,
    /// The body size the type ladder is scaled against, in points.
    pub text_size: f32,
    /// The hue the greys carry, and how much of it.
    pub tint: Tint,
    /// Whether the window is showing the frame meter. Runtime only — a switch
    /// you left on is not a preference worth restoring.
    pub meter: bool,
    /// The registry's mark for each configured agent, by name. Empty until the
    /// catalog lands, and stays empty offline.
    agent_icons: HashMap<String, SharedString>,
}

impl Workspace {
    pub fn new(settings: Settings, state: State, cx: &mut Context<Self>) -> Self {
        let projects: Vec<Project> = state.projects.into_iter().map(Project::new).collect();
        let active = (!projects.is_empty()).then_some(state.active);
        let restore: Vec<usize> = (0..projects.len()).collect();
        let mut this = Self {
            settings,
            projects,
            active,
            appearance: state.appearance,
            reduce_transparency: state.reduce_transparency,
            cursor_blink: state.cursor_blink,
            text_size: state.text_size,
            tint: Tint::new(state.hue, state.chroma),
            meter: false,
            next_id: 0,
            agent_icons: HashMap::new(),
        };
        for ix in restore {
            this.restore_sessions(ix);
        }
        this.open_last_session(cx);
        this.load_agent_icons(cx);
        // Temporary dev hook: `CYDONIA_TEST_PROMPT` sends a prompt on launch
        // so a turn can be verified without a composer. Here rather than on
        // connect, which a resume would fire again.
        if let Ok(prompt) = std::env::var("CYDONIA_TEST_PROMPT") {
            let id = this.active_id().or_else(|| {
                let entry = this.settings.agents.first().cloned()?;
                this.new_session(entry, None, cx)
            });
            if let Some(id) = id {
                this.send(id, prompt, cx);
            }
        }
        this
    }

    fn save(&self) {
        state::save(&State {
            projects: self.projects.iter().map(|p| p.path.clone()).collect(),
            active: self.active.unwrap_or_default(),
            appearance: self.appearance,
            reduce_transparency: self.reduce_transparency,
            cursor_blink: self.cursor_blink,
            text_size: self.text_size,
            hue: self.tint.hue,
            chroma: self.tint.chroma,
        });
    }

    // ── agents ───────────────────────────────────────────────────────

    /// Fetch the catalog and keep each configured agent's icon. Off the UI
    /// thread — the registry is a blocking fetch on a cold cache — and a
    /// failure just leaves the map empty.
    fn load_agent_icons(&mut self, cx: &mut Context<Self>) {
        let configured = self.settings.agents.clone();
        cx.spawn(async move |this, cx| {
            let icons = cx
                .background_executor()
                .spawn(async move { agent::icons(&configured) })
                .await;
            let _ = this.update(cx, |workspace, cx| {
                workspace.agent_icons = icons;
                cx.notify();
            });
        })
        .detach();
    }

    /// The registry's mark for whatever this session runs on.
    pub fn agent_icon(&self, name: &str) -> Option<SharedString> {
        self.agent_icons.get(name).cloned()
    }

    /// Which agent an unasked-for session runs on: whoever the project is
    /// already talking to, else the first one configured.
    pub fn preferred_agent(&self) -> Option<settings::Agent> {
        self.active_session()
            .map(|chat| chat.entry.clone())
            .or_else(|| self.settings.agents.first().cloned())
    }

    /// Re-read `settings.toml`. Installing an agent writes that file, and
    /// reading it back is what keeps this list identical to it — cheaper than
    /// a second copy of the rule for which entry an install replaces.
    pub fn reload_settings(&mut self, cx: &mut Context<Self>) {
        if let Ok(settings) = settings::load() {
            self.settings = settings;
        }
        cx.notify();
    }

    /// The settings window's choice. bezel repaints on `set_mode`; the state
    /// file is what makes it survive a relaunch.
    pub fn set_appearance(&mut self, mode: AppearanceMode, cx: &mut Context<Self>) {
        self.appearance = mode;
        bezel::theme::appearance::set_mode(mode, cx);
        self.save();
        cx.notify();
    }

    /// The same window's other choice.
    pub fn set_reduce_transparency(&mut self, reduce: bool, cx: &mut Context<Self>) {
        self.reduce_transparency = reduce;
        apply_transparency(reduce, cx);
        self.save();
        cx.notify();
    }

    /// The caret is bezel's, so the setting is: nothing here reads it back.
    pub fn set_cursor_blink(&mut self, blink: bool, cx: &mut Context<Self>) {
        self.cursor_blink = blink;
        input::set_caret_blink(blink, cx);
        self.save();
        cx.notify();
    }

    pub fn set_text_size(&mut self, points: f32, cx: &mut Context<Self>) {
        self.text_size = points;
        theme::set_base_text_size(points, cx);
        self.save();
        cx.notify();
    }

    pub fn set_tint(&mut self, tint: Tint, cx: &mut Context<Self>) {
        self.tint = tint;
        apply_tint(tint, cx);
        self.save();
        cx.notify();
    }

    // ── projects ─────────────────────────────────────────────────────

    /// A project just added starts talking to an agent; one already on the
    /// rail is only brought forward.
    pub fn open_project(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        if let Some(ix) = self.projects.iter().position(|p| p.path == path) {
            self.select_project(ix, cx);
            return;
        }
        self.projects.push(Project::new(path));
        let ix = self.projects.len() - 1;
        self.restore_sessions(ix);
        self.select_project(ix, cx);
        if self.projects[ix].sessions.is_empty()
            && let Some(entry) = self.settings.agents.first().cloned()
        {
            self.new_session(entry, None, cx);
        }
    }

    pub fn select_project(&mut self, ix: usize, cx: &mut Context<Self>) {
        if ix >= self.projects.len() {
            return;
        }
        self.active = Some(ix);
        self.open_last_session(cx);
        self.save();
        cx.notify();
    }

    /// Drop the project: its sessions go with it, and each session's shutdown
    /// sender goes with that — the agent processes die here.
    pub fn close_project(&mut self, ix: usize, cx: &mut Context<Self>) {
        if ix >= self.projects.len() {
            return;
        }
        self.projects.remove(ix);
        self.active = self.active.and_then(|active| {
            let next = if active > ix { active - 1 } else { active };
            (!self.projects.is_empty()).then(|| next.min(self.projects.len() - 1))
        });
        self.open_last_session(cx);
        self.save();
        cx.notify();
    }

    /// The project in front shows its most recent session; nothing there is
    /// connected until something is sent to it.
    fn open_last_session(&mut self, cx: &mut Context<Self>) {
        let Some(project) = self.active.and_then(|ix| self.projects.get_mut(ix)) else {
            return;
        };
        if project.active.is_some() {
            return;
        }
        if let Some(id) = project.sessions.last().map(|chat| chat.id) {
            project.active = Some(id);
            cx.notify();
        }
    }

    pub fn active_project(&self) -> Option<&Project> {
        self.active.and_then(|ix| self.projects.get(ix))
    }

    pub fn active_project_mut(&mut self) -> Option<&mut Project> {
        let ix = self.active?;
        self.projects.get_mut(ix)
    }

    // ── sessions ─────────────────────────────────────────────────────

    /// Open a session in the active project. `seed` is its first prompt, sent
    /// as soon as the agent is up — what a dispatched card rides in on.
    pub fn new_session(
        &mut self,
        entry: settings::Agent,
        seed: Option<String>,
        cx: &mut Context<Self>,
    ) -> Option<u64> {
        let ix = self.active?;
        let id = self.next_id;
        self.next_id += 1;
        let chat = ChatSession::connect(id, entry, self.projects[ix].path.clone(), seed, cx);
        let project = &mut self.projects[ix];
        project.sessions.push(chat);
        project.active = Some(id);
        cx.notify();
        Some(id)
    }

    /// Every project's sessions are on show, so picking one brings its project
    /// forward with it.
    pub fn select_session(&mut self, id: u64, cx: &mut Context<Self>) {
        let Some(ix) = self.project_of(id) else {
            return;
        };
        self.projects[ix].active = Some(id);
        if self.active != Some(ix) {
            self.active = Some(ix);
            self.save();
        }
        cx.notify();
    }

    fn project_of(&self, id: u64) -> Option<usize> {
        self.projects
            .iter()
            .position(|project| project.session(id).is_some())
    }

    /// Read the project's filed sessions back, minting an id for each — ids
    /// mean nothing across a launch, so a reloaded one is as new as any. The
    /// agent is resolved by name; a session whose agent has since left
    /// `settings.toml` comes back readable but cannot reconnect.
    fn restore_sessions(&mut self, ix: usize) {
        let path = self.projects[ix].path.clone();
        for (file, stored) in record::list(&path) {
            let id = self.next_id;
            self.next_id += 1;
            let entry = self
                .settings
                .agents
                .iter()
                .find(|agent| agent.name == stored.agent)
                .cloned()
                .unwrap_or_else(|| settings::Agent {
                    name: stored.agent.clone(),
                    id: None,
                    command: String::new(),
                    args: Vec::new(),
                    env: Default::default(),
                });
            let chat = ChatSession::restore(id, file, path.clone(), entry, stored);
            self.projects[ix].sessions.push(chat);
        }
    }

    /// Send to a session, starting an agent for it when it has none — typing
    /// into a session read back from disk is what picks it up again.
    pub fn send(&mut self, id: u64, content: String, cx: &mut Context<Self>) {
        let found = self
            .projects
            .iter_mut()
            .find_map(|project| project.session_mut(id));
        let Some(chat) = found else {
            return;
        };
        if chat.idle() && chat.resumable() {
            chat.resume(cx);
        }
        chat.send(content);
        cx.notify();
    }

    /// Close the connection and keep the transcript. The row stays where it
    /// was, readable, and typing into it opens an agent again.
    pub fn archive_session(&mut self, id: u64, cx: &mut Context<Self>) {
        self.with_session(id, cx, |chat| chat.close());
    }

    pub fn rename_session(&mut self, id: u64, name: String, cx: &mut Context<Self>) {
        self.with_session(id, cx, |chat| {
            let name = name.trim();
            chat.name = (!name.is_empty()).then(|| name.to_owned());
            chat.flush();
        });
    }

    pub fn close_session(&mut self, id: u64, cx: &mut Context<Self>) {
        let Some(project) = self.project_of(id).map(|ix| &mut self.projects[ix]) else {
            return;
        };
        // Closing a session is what deletes it: leaving the file would put
        // the row back on the next launch.
        if let Some(file) = project.session(id).and_then(|chat| chat.file.as_ref()) {
            record::remove(file);
        }
        project.sessions.retain(|chat| chat.id != id);
        if project.active == Some(id) {
            project.active = project.sessions.last().map(|chat| chat.id);
        }
        cx.notify();
    }

    /// Any session, in whichever project holds it — the pump that feeds a
    /// session knows only its id, and must not care which tab it sits behind.
    pub fn session(&self, id: u64) -> Option<&ChatSession> {
        self.projects.iter().find_map(|project| project.session(id))
    }

    /// Run `f` on the session (when it still exists) and repaint.
    pub fn with_session(
        &mut self,
        id: u64,
        cx: &mut Context<Self>,
        f: impl FnOnce(&mut ChatSession),
    ) {
        let found = self
            .projects
            .iter_mut()
            .find_map(|project| project.session_mut(id));
        if let Some(chat) = found {
            f(chat);
            cx.notify();
        }
    }

    /// The session reached an agent: send it whatever was typed while it had
    /// none, and write the agent's own id down so a later launch can load the
    /// conversation back.
    pub fn session_connected(&mut self, id: u64, cx: &mut Context<Self>) {
        self.with_session(id, cx, |chat| {
            chat.drain();
            chat.flush();
        });
        cx.notify();
    }

    pub fn active_session(&self) -> Option<&ChatSession> {
        self.active_project().and_then(Project::active_session)
    }

    pub fn active_id(&self) -> Option<u64> {
        self.active_project().and_then(|project| project.active)
    }

    // ── boards ───────────────────────────────────────────────────────

    /// A fresh board in the active project, opened as it lands.
    pub fn new_board(&mut self, cx: &mut Context<Self>) -> Option<usize> {
        let project = self.active?;
        let board = board::create(&self.projects[project].path)?;
        self.projects[project].boards.push(board);
        let ix = self.projects[project].boards.len() - 1;
        self.open_board(project, ix, cx);
        Some(ix)
    }

    /// Every project's boards are on show, so picking one brings its project
    /// forward with it.
    pub fn open_board(&mut self, project: usize, ix: usize, cx: &mut Context<Self>) {
        let Some(open) = self.projects.get_mut(project) else {
            return;
        };
        if ix >= open.boards.len() {
            return;
        }
        open.board = Some(ix);
        if self.active != Some(project) {
            self.active = Some(project);
            self.save();
        }
        cx.notify();
    }

    /// Drop the board: the file goes with it.
    pub fn delete_board(&mut self, project: usize, ix: usize, cx: &mut Context<Self>) {
        let Some(project) = self.projects.get_mut(project) else {
            return;
        };
        if ix >= project.boards.len() {
            return;
        }
        project.boards.remove(ix).remove();
        project.board = project
            .board
            .filter(|open| *open != ix)
            .map(|open| if open > ix { open - 1 } else { open });
        cx.notify();
    }

    pub fn rename_board(
        &mut self,
        project: usize,
        ix: usize,
        name: String,
        cx: &mut Context<Self>,
    ) {
        let Some(board) = self
            .projects
            .get_mut(project)
            .and_then(|open| open.boards.get_mut(ix))
        else {
            return;
        };
        board.name = name.trim().to_owned();
        board.save();
        cx.notify();
    }

    pub fn active_board(&self) -> Option<&Board> {
        let project = self.active_project()?;
        project.boards.get(project.board?)
    }

    pub fn active_board_mut(&mut self) -> Option<&mut Board> {
        let project = self.projects.get_mut(self.active?)?;
        project.boards.get_mut(project.board?)
    }

    // ── articles ─────────────────────────────────────────────────────

    /// A fresh document in the active project, opened as it lands — an empty
    /// article has nothing to look at but the caret.
    pub fn new_article(&mut self, cx: &mut Context<Self>) -> Option<usize> {
        let project = self.active?;
        let article = article::create(&self.projects[project].path)?;
        self.projects[project].articles.push(article);
        let ix = self.projects[project].articles.len() - 1;
        self.open_article(project, ix, cx);
        Some(ix)
    }

    /// Every project's articles are on show, so picking one brings its project
    /// forward with it.
    pub fn open_article(&mut self, project: usize, ix: usize, cx: &mut Context<Self>) {
        let Some(article) = self
            .projects
            .get_mut(project)
            .and_then(|open| open.articles.get_mut(ix))
        else {
            return;
        };
        article.open(cx);
        self.projects[project].article = Some(ix);
        if self.active != Some(project) {
            self.active = Some(project);
            self.save();
        }
        cx.notify();
    }

    /// Drop the article: the file goes with it.
    pub fn delete_article(&mut self, project: usize, ix: usize, cx: &mut Context<Self>) {
        let Some(project) = self.projects.get_mut(project) else {
            return;
        };
        if ix >= project.articles.len() {
            return;
        }
        project.articles.remove(ix).remove();
        project.article = project
            .article
            .filter(|open| *open != ix)
            .map(|open| if open > ix { open - 1 } else { open });
        cx.notify();
    }

    pub fn active_article(&self) -> Option<&Article> {
        let project = self.active_project()?;
        project.articles.get(project.article?)
    }

    /// Put a cover on the open article, or take it off — see
    /// [`Article::set_cover`].
    pub fn set_cover(&mut self, source: Option<&Path>, cx: &mut Context<Self>) {
        if let Some(article) = self.article_mut() {
            article.set_cover(source);
            cx.notify();
        }
    }

    /// Cut the open article a new cover — see [`Article::shuffle_cover`].
    pub fn shuffle_cover(&mut self, cx: &mut Context<Self>) {
        if let Some(article) = self.article_mut() {
            article.shuffle_cover();
            cx.notify();
        }
    }

    fn article_mut(&mut self) -> Option<&mut Article> {
        let project = self.projects.get_mut(self.active?)?;
        project.articles.get_mut(project.article?)
    }

    // ── tables ───────────────────────────────────────────────────────

    /// A fresh table in the active project, opened as it lands.
    ///
    /// One text column, because the store will not make a table without one
    /// and a column you can rename is a better start than a dialog asking for
    /// the shape before anything exists to shape.
    pub fn new_table(&mut self, cx: &mut Context<Self>) -> Option<usize> {
        let at = self.active?;
        let project = self.projects.get_mut(at)?;
        // The one place a store is created: making a table is the moment the
        // project has something to keep in one.
        if project.data.is_none() {
            project.data = Data::open(&project.path).ok();
        }
        let mut name = UNTITLED.to_owned();
        for n in 2.. {
            if !project.tables.iter().any(|table| table.name == name) {
                break;
            }
            name = format!("{UNTITLED} {n}");
        }
        let column = Column {
            name: "Name".to_owned(),
            kind: ColType::Text,
        };
        let key = project
            .data
            .as_mut()?
            .create(&name, None, &[column], None)
            .ok()?
            .key;
        project.reload_tables();
        // Found by key rather than taken as the last row: the list is ordered,
        // so a new table lands wherever its name sorts.
        let ix = project.tables.iter().position(|table| table.key == key)?;
        self.open_table(at, ix, cx);
        Some(ix)
    }

    /// Every project's tables are on show, so picking one brings its project
    /// forward with it.
    pub fn open_table(&mut self, project: usize, ix: usize, cx: &mut Context<Self>) {
        let Some(open) = self.projects.get_mut(project) else {
            return;
        };
        if ix >= open.tables.len() {
            return;
        }
        open.table = Some(ix);
        open.reload_page();
        if self.active != Some(project) {
            self.active = Some(project);
            self.save();
        }
        cx.notify();
    }

    /// Drop the table: its rows go with it.
    pub fn delete_table(&mut self, project: usize, ix: usize, cx: &mut Context<Self>) {
        let Some(open) = self.projects.get_mut(project) else {
            return;
        };
        let Some(key) = open.tables.get(ix).map(|table| table.key.clone()) else {
            return;
        };
        if let Some(data) = open.data.as_mut() {
            let _ = data.remove(&key);
        }
        open.table = open
            .table
            .filter(|shown| *shown != ix)
            .map(|shown| if shown > ix { shown - 1 } else { shown });
        open.reload_tables();
        cx.notify();
    }

    /// Run `f` against the open table's store, then re-read what it did.
    ///
    /// Every table mutation goes through here, so none of them can forget the
    /// reload — a grid still showing the row you just deleted is the bug this
    /// shape makes unwritable.
    fn with_table<T>(
        &mut self,
        cx: &mut Context<Self>,
        f: impl FnOnce(&mut Data, &str) -> T,
    ) -> Option<T> {
        let at = self.active?;
        let project = self.projects.get_mut(at)?;
        let key = project
            .table
            .and_then(|ix| project.tables.get(ix))
            .map(|table| table.key.clone())?;
        let done = f(project.data.as_mut()?, &key);
        project.reload_tables();
        cx.notify();
        Some(done)
    }

    /// The name of column `at`, which is what the store addresses one by.
    fn column_name(&self, at: usize) -> Option<String> {
        let page = self.active_page()?;
        page.columns.get(at).map(|column| column.name.clone())
    }

    /// Write one cell. The text goes in as text whatever the column holds —
    /// SQLite's affinity converts it on the way, so a number typed into a
    /// number column lands as one and the same text in a text column stays put.
    pub fn write_cell(&mut self, rowid: i64, at: usize, text: String, cx: &mut Context<Self>) {
        let Some(column) = self.column_name(at) else {
            return;
        };
        let value = match text.is_empty() {
            true => serde_json::Value::Null,
            false => serde_json::Value::String(text),
        };
        self.with_table(cx, |data, key| {
            let _ = data.write_cells(
                key,
                &[Edit {
                    rowid,
                    column,
                    value,
                }],
            );
        });
    }

    pub fn add_row(&mut self, cx: &mut Context<Self>) -> Option<i64> {
        self.with_table(cx, |data, key| {
            data.add_rows(key, 1)
                .ok()
                .and_then(|ids| ids.first().copied())
        })
        .flatten()
    }

    pub fn delete_row(&mut self, rowid: i64, cx: &mut Context<Self>) {
        self.with_table(cx, |data, key| {
            let _ = data.delete_rows(key, &[rowid]);
        });
    }

    /// A fresh text column, named so it does not collide with one already
    /// there — the header is where it gets its real name.
    pub fn add_column(&mut self, cx: &mut Context<Self>) {
        let taken: Vec<String> = self
            .active_page()
            .map(|page| page.columns.iter().map(|col| col.name.clone()).collect())
            .unwrap_or_default();
        let mut name = COLUMN.to_owned();
        for n in 2.. {
            if !taken.contains(&name) {
                break;
            }
            name = format!("{COLUMN} {n}");
        }
        self.with_table(cx, |data, key| {
            let _ = data.write_column(key, &name, Some(ColType::Text), None);
        });
    }

    /// Rename column `at`, retype it, or both — one call, as the store has it.
    pub fn write_column(
        &mut self,
        at: usize,
        kind: Option<ColType>,
        rename: Option<String>,
        cx: &mut Context<Self>,
    ) {
        let Some(column) = self.column_name(at) else {
            return;
        };
        self.with_table(cx, |data, key| {
            let _ = data.write_column(key, &column, kind, rename.as_deref());
        });
    }

    pub fn delete_column(&mut self, at: usize, cx: &mut Context<Self>) {
        let Some(column) = self.column_name(at) else {
            return;
        };
        self.with_table(cx, |data, key| {
            let _ = data.drop_column(key, &column);
        });
    }

    /// The display name only. The key stays where it is, so a query already
    /// written against this table goes on running.
    pub fn rename_table(&mut self, name: String, cx: &mut Context<Self>) {
        self.with_table(cx, |data, key| {
            let _ = data.update(key, Some(&name), None);
        });
    }

    /// The open table's rows, as the pane last read them.
    pub fn active_page(&self) -> Option<&Page> {
        self.active_project()?.page.as_ref()
    }

    pub fn active_table(&self) -> Option<&Table> {
        let project = self.active_project()?;
        project.tables.get(project.table?)
    }

    /// The title or the content changed. Found by the entity because an article
    /// has two surfaces and either can be the one that moved.
    pub fn write_article(&mut self, changed: EntityId, cx: &mut Context<Self>) {
        let found = self.projects.iter_mut().find_map(|project| {
            project.articles.iter_mut().find(|article| {
                article
                    .field
                    .as_ref()
                    .is_some_and(|field| field.entity_id() == changed)
                    || article
                        .editor
                        .as_ref()
                        .is_some_and(|editor| editor.entity_id() == changed)
            })
        });
        if found.is_some_and(|article| article.write(cx)) {
            cx.notify();
        }
    }
}

/// Point bezel's frost alpha at the preference. Free rather than a method
/// because the window reads its background appearance while it is being opened,
/// which is before there is a workspace to ask.
pub fn apply_tint(tint: Tint, cx: &mut App) {
    theme::set_brand(
        Brand {
            tint,
            ..theme::brand(cx)
        },
        cx,
    );
}

/// Two answers, because bezel asks two questions: the window stops compositing
/// translucent, and the frost over it goes opaque. Chrome keeps its layers —
/// an opaque window carrying them is what this setting asks for, and what the
/// system's own does not do.
pub fn apply_transparency(reduce: bool, cx: &mut App) {
    let glass = if reduce { 1.0 } else { Theme::GLASS_ALPHA };
    theme::set_brand(
        Brand {
            glass,
            vibrancy: !reduce,
            ..theme::brand(cx)
        },
        cx,
    );
}
