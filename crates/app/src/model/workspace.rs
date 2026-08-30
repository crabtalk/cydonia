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
    agents,
    model::{
        archive,
        article::{self, Article},
        board::Board,
        project::Project,
        session::ChatSession,
        settings::{self, Settings},
        state::{self, State},
    },
};
use bezel::{
    gpui::{Context, EntityId, SharedString},
    theme::appearance::AppearanceMode,
};
use std::{collections::HashMap, path::PathBuf};

pub struct Workspace {
    pub settings: Settings,
    pub projects: Vec<Project>,
    pub active: Option<usize>,
    pub appearance: AppearanceMode,
    /// Session ids are minted here and never reused, so a card's link to the
    /// session it opened stays unambiguous for the life of the process.
    next_id: u64,
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
            next_id: 0,
            agent_icons: HashMap::new(),
        };
        for ix in restore {
            this.restore_archived(ix, cx);
        }
        this.open_first_session(cx);
        this.load_agent_icons(cx);
        this
    }

    fn save(&self) {
        state::save(&self.projects, self.active, self.appearance);
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
                .spawn(async move { agents::icons(&configured) })
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

    // ── projects ─────────────────────────────────────────────────────

    pub fn open_project(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        let open = self.projects.iter().position(|p| p.path == path);
        let ix = match open {
            Some(ix) => ix,
            None => {
                self.projects.push(Project::new(path));
                let ix = self.projects.len() - 1;
                self.restore_archived(ix, cx);
                ix
            }
        };
        self.select_project(ix, cx);
    }

    pub fn select_project(&mut self, ix: usize, cx: &mut Context<Self>) {
        if ix >= self.projects.len() {
            return;
        }
        self.active = Some(ix);
        self.open_first_session(cx);
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
        self.open_first_session(cx);
        self.save();
        cx.notify();
    }

    /// A project talks to an agent the moment it is looked at: the tab in
    /// front opens its first session, and the tabs behind it spawn nothing.
    fn open_first_session(&mut self, cx: &mut Context<Self>) {
        if self
            .active_project()
            .is_none_or(|project| !project.sessions.is_empty())
        {
            return;
        }
        if let Some(entry) = self.settings.agents.first().cloned() {
            self.new_session(entry, None, cx);
        }
    }

    pub fn active_project(&self) -> Option<&Project> {
        self.active.and_then(|ix| self.projects.get(ix))
    }

    pub fn active_board_mut(&mut self) -> Option<&mut Board> {
        let ix = self.active?;
        Some(&mut self.projects.get_mut(ix)?.board)
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

    /// Drop the session: the shutdown sender goes with it and the agent
    /// process dies.
    /// Read the project's filed transcripts back, minting an id for each —
    /// ids mean nothing across a launch, so a reloaded one is as new as any.
    fn restore_archived(&mut self, ix: usize, cx: &mut Context<Self>) {
        let path = self.projects[ix].path.clone();
        for (file, record) in archive::list(&path) {
            let id = self.next_id;
            self.next_id += 1;
            let chat = ChatSession::from_archive(id, file, record, cx);
            self.projects[ix].sessions.push(chat);
        }
    }

    /// File the transcript, then close the connection behind it. The row stays
    /// where it was, readable — an archive you cannot open is a delete.
    pub fn archive_session(&mut self, id: u64, cx: &mut Context<Self>) {
        let Some(ix) = self.project_of(id) else {
            return;
        };
        let path = self.projects[ix].path.clone();
        let Some(chat) = self.projects[ix].session_mut(id) else {
            return;
        };
        if chat.archive.is_some() {
            return;
        }
        let Some(file) = archive::write(&path, &chat.to_archive()) else {
            return;
        };
        chat.close(file);
        cx.notify();
    }

    pub fn rename_session(&mut self, id: u64, name: String, cx: &mut Context<Self>) {
        self.with_session(id, cx, |chat| {
            let name = name.trim();
            chat.name = (!name.is_empty()).then(|| name.to_owned());
        });
    }

    pub fn close_session(&mut self, id: u64, cx: &mut Context<Self>) {
        let Some(project) = self.project_of(id).map(|ix| &mut self.projects[ix]) else {
            return;
        };
        // Closing an archived session is what deletes it: leaving the file
        // would put the row back on the next launch.
        if let Some(file) = project.session(id).and_then(|chat| chat.archive.as_ref()) {
            archive::remove(file);
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

    /// The session opened. Temporary dev hook: `CYDONIA_TEST_PROMPT` sends
    /// a prompt right away so streaming can be verified without a composer.
    pub fn session_connected(&mut self, id: u64, cx: &mut Context<Self>) {
        self.with_session(id, cx, |chat| {
            if let Some(seed) = chat.seed.take() {
                chat.send(seed);
            }
        });
        if let Ok(prompt) = std::env::var("CYDONIA_TEST_PROMPT") {
            self.with_session(id, cx, |chat| chat.send(prompt));
        }
        cx.notify();
    }

    pub fn active_session(&self) -> Option<&ChatSession> {
        self.active_project().and_then(Project::active_session)
    }

    pub fn active_id(&self) -> Option<u64> {
        self.active_project().and_then(|project| project.active)
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

    /// Settle the open article's name — see [`Article::rename`]. Called on the
    /// way out of an article, which is the moment its title is finished.
    pub fn rename_article(&mut self, cx: &mut Context<Self>) {
        let Some(project) = self.active.and_then(|at| self.projects.get_mut(at)) else {
            return;
        };
        let Some(article) = project.article.and_then(|ix| project.articles.get_mut(ix)) else {
            return;
        };
        article.rename();
        cx.notify();
    }

    pub fn active_article(&self) -> Option<&Article> {
        let project = self.active_project()?;
        project.articles.get(project.article?)
    }

    /// The editor changed. Found by the entity rather than by a path, because
    /// a document that has just been given a title has moved.
    pub fn write_article(&mut self, editor: EntityId, source: String) {
        let found = self.projects.iter_mut().find_map(|project| {
            project.articles.iter_mut().find(|article| {
                article
                    .editor
                    .as_ref()
                    .is_some_and(|open| open.entity_id() == editor)
            })
        });
        if let Some(article) = found {
            article.write(source);
        }
    }
}
