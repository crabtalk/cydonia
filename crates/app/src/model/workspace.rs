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
        board::Board,
        project::Project,
        session::ChatSession,
        settings::{self, Settings},
        state::{self, State},
    },
};
use bezel::{gpui::SharedString, theme::appearance::AppearanceMode};
use gpui::Context;
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
        let mut this = Self {
            settings,
            projects,
            active,
            appearance: state.appearance,
            next_id: 0,
            agent_icons: HashMap::new(),
        };
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
                self.projects.len() - 1
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
    pub fn close_session(&mut self, id: u64, cx: &mut Context<Self>) {
        let Some(project) = self.project_of(id).map(|ix| &mut self.projects[ix]) else {
            return;
        };
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
}
