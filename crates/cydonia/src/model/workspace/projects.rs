//! The projects on the rail: opening, ordering, closing, and the watch
//! each one carries.
//!
//! A continuation of [`Workspace`]'s one `impl`, which is why it opens on
//! `use super::*`: these methods work on the same struct and reach the same
//! names as the rest of it.
use super::*;

impl Workspace {
    /// The projects on the rail, in the order the sidebar lists them.
    pub(super) fn paths(&self) -> Vec<PathBuf> {
        self.projects.iter().map(|open| open.path.clone()).collect()
    }

    /// Take what the tools ask of the rail — see [`mcp::rail`], where the other
    /// half of this is written.
    ///
    /// Installed once and drained here rather than acted on where it arrives: a
    /// tool call lands on whichever thread the door is serving from, and a
    /// project can only be opened where the app's own state is. The channel is
    /// what carries it across.
    pub(super) fn take_rail(&self, cx: &mut Context<Self>) {
        let (asked, mut asks) = mpsc::unbounded();
        rail::install(move |change| {
            let _ = asked.unbounded_send(change);
        });
        rail::set_open(self.paths());
        cx.spawn(async move |workspace, cx| {
            while let Some(change) = asks.next().await {
                let held = workspace
                    .update(cx, |workspace, cx| match change {
                        Change::Open(path) => workspace.open_project_at(path, cx),
                        Change::Close(path) => workspace.close_project_at(&path, cx),
                    })
                    .is_ok();
                // The workspace has gone, and there is no rail to move.
                if !held {
                    return;
                }
            }
        })
        .detach();
    }

    /// Put a project on the rail, or bring forward one already there.
    ///
    /// Whatever sessions it has on disk come back with it, and no more than
    /// that: adding a project used to open one on the first configured agent,
    /// which spawned a process — a download, on an `npx` line — for somebody
    /// who had done nothing but name a folder. A session is opened where one is
    /// asked for, and ⌘N is where.
    pub fn open_project(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        if let Some(ix) = self.projects.iter().position(|p| p.path == path) {
            self.select_project(ix, cx);
            return;
        }
        self.projects.push(Project::new(path));
        let ix = self.projects.len() - 1;
        self.restore_sessions(ix);
        self.watch_project(ix, cx);
        self.select_project(ix, cx);
    }

    pub fn select_project(&mut self, ix: usize, cx: &mut Context<Self>) {
        if ix >= self.projects.len() {
            return;
        }
        self.active = Some(ix);
        self.open_last_entry(cx);
        self.prune_archived(cx);
        self.save();
        cx.notify();
    }

    pub fn toggle_project(&mut self, ix: usize, cx: &mut Context<Self>) {
        let Some(project) = self.projects.get_mut(ix) else {
            return;
        };
        project.expanded = !project.expanded;
        self.save();
        cx.notify();
    }

    /// Carry a project to another place in the list. `active` follows the
    /// project it points at rather than the index it sits on: which one is in
    /// front has nothing to do with what order they are listed in.
    pub fn move_project(&mut self, from: usize, to: usize, cx: &mut Context<Self>) {
        if from == to || from >= self.projects.len() || to >= self.projects.len() {
            return;
        }
        let project = self.projects.remove(from);
        self.projects.insert(to, project);
        self.active = self.active.map(|at| match at {
            at if at == from => to,
            at if from < to && (from..=to).contains(&at) => at - 1,
            at if to < from && (to..=from).contains(&at) => at + 1,
            at => at,
        });
        self.save();
        cx.notify();
    }

    /// Whichever project is at `path`, by the path the sidebar holds it under
    /// or by what that resolves to.
    ///
    /// The tools settle a path before handing it over and the directory picker
    /// does not — see `mcp::tools::project` — so `/tmp/x` on the rail and
    /// `/private/tmp/x` from a tool are one project, and opening the second
    /// would otherwise list the same directory twice.
    fn project_at(&self, path: &Path) -> Option<usize> {
        self.projects.iter().position(|open| {
            open.path == path
                || open
                    .path
                    .canonicalize()
                    .is_ok_and(|settled| settled == path)
        })
    }

    /// Bring the project at `path` forward, opening it where it is not on the
    /// rail at all. What the tools ask for; the sidebar names a row instead.
    fn open_project_at(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        match self.project_at(&path) {
            Some(ix) => self.select_project(ix, cx),
            None => self.open_project(path, cx),
        }
    }

    /// Close whichever project is at `path`, if one is.
    fn close_project_at(&mut self, path: &Path, cx: &mut Context<Self>) {
        let Some(ix) = self.project_at(path) else {
            return;
        };
        self.close_project(ix, cx);
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
        self.open_last_entry(cx);
        self.prune_archived(cx);
        self.save();
        cx.notify();
    }

    /// Put the project in front back where it was left — the entry it was last
    /// showing, of whichever kind. Nothing is connected by it: a session opened
    /// this way stays idle until something is sent to it.
    ///
    /// The remembered entry is found by its own identity rather than by
    /// position, because a sibling added or removed between launches shifts
    /// every index after it.
    pub(super) fn open_last_entry(&mut self, cx: &mut Context<Self>) {
        let Some(ix) = self.active else {
            return;
        };
        let Some(entry) = self
            .projects
            .get(ix)
            .and_then(|open| self.last.get(&open.path))
        else {
            return;
        };
        let (kind, id) = (entry.kind, entry.id.clone());
        let text_size = self.article_font_size();
        let Some(project) = self.projects.get_mut(ix) else {
            return;
        };
        let at = |path: Option<&Path>| path.is_some_and(|path| path.to_string_lossy() == id);
        match kind {
            state::Kind::Session => {
                project.active = project
                    .sessions
                    .iter()
                    .find(|chat| chat.record.as_deref() == Some(id.as_str()))
                    .map(|chat| chat.id);
            }
            state::Kind::Board => {
                project.board = project.boards.iter().position(|board| board.id == id);
            }
            state::Kind::Article => {
                project.article = project
                    .articles
                    .iter()
                    .position(|article| at(Some(&article.path)));
                if let Some(at) = project.article {
                    project.articles[at].open(text_size, cx);
                }
            }
            state::Kind::Table => {
                project.table = project.tables.iter().position(|table| table.key == id);
                project.reload_page();
            }
        }
        // Landing back in a session is being in front of it — see
        // [`Self::wake_session`]. Landing in an article or a board is not, and
        // starts nothing.
        let woken = self
            .projects
            .get(ix)
            .filter(|_| matches!(kind, state::Kind::Session))
            .and_then(|open| open.active);
        if let Some(id) = woken {
            self.wake_session(id, cx);
        }
        cx.notify();
    }

    /// What the active project was last showing, by kind. What the window puts
    /// its pane on when it lands here — [`Self::open_last_entry`] opens the
    /// entry, and this is how the view learns which one of the four it was.
    pub fn landing(&self) -> Option<state::Kind> {
        let open = self.active_project()?;
        self.last.get(&open.path).map(|entry| entry.kind)
    }

    /// Remember the entry a project is now showing, so the next launch lands on
    /// it. Every way of opening one arrives here.
    pub(super) fn remember(
        &mut self,
        project: usize,
        kind: state::Kind,
        id: String,
        cx: &mut Context<Self>,
    ) {
        let Some(open) = self.projects.get(project) else {
            return;
        };
        self.last
            .insert(open.path.clone(), state::Entry { kind, id });
        self.prune_archived(cx);
        self.save();
    }

    pub(super) fn prune_archived(&mut self, cx: &mut Context<Self>) {
        self.prune_archived_for(self.landing(), cx);
    }

    pub(super) fn prune_archived_for(&mut self, kind: Option<state::Kind>, cx: &mut Context<Self>) {
        for (ix, project) in self.projects.iter_mut().enumerate() {
            let active = self.active == Some(ix);
            for chat in &mut project.sessions {
                if !(active
                    && kind == Some(state::Kind::Session)
                    && project.active == Some(chat.id))
                {
                    chat.unload_history();
                }
            }
            for (at, article) in project.articles.iter_mut().enumerate() {
                if !(active && kind == Some(state::Kind::Article) && project.article == Some(at)) {
                    article.unload(cx);
                }
            }
            project.unload_boards(if active && kind == Some(state::Kind::Board) {
                project.board
            } else {
                None
            });
            if !(active && kind == Some(state::Kind::Table))
                && project
                    .table
                    .and_then(|at| project.tables.get(at))
                    .is_some_and(|table| table.archived)
            {
                project.page = None;
            }
        }
    }

    pub fn active_project(&self) -> Option<&Project> {
        self.active.and_then(|ix| self.projects.get(ix))
    }

    // ── watching ─────────────────────────────────────────────────────

    /// Put a watch on the project at `ix`, so what an agent writes into it
    /// shows up without anyone asking for it. Every way of opening a project
    /// arrives here, and closing one drops the watch with the project.
    pub(super) fn watch_project(&mut self, ix: usize, cx: &mut Context<Self>) {
        let Some(path) = self.projects.get(ix).map(|open| open.path.clone()) else {
            return;
        };
        let watch = Watch::open(path, cx);
        if let Some(project) = self.projects.get_mut(ix) {
            project.watch = Some(watch);
        }
    }

    /// Re-read one project off disk and reconcile it. Addressed by path rather
    /// than by index because the watch that calls this outlives any index it
    /// could have been armed with — the rail is reorderable, and closing a
    /// project shifts every one after it.
    pub fn reload_project(&mut self, path: &Path, cx: &mut Context<Self>) {
        let Some(ix) = self.projects.iter().position(|open| open.path == path) else {
            return;
        };
        if self.projects[ix].reload(cx) {
            cx.emit(Reloaded);
        }
        self.prune_archived(cx);
        cx.notify();
    }

    /// Re-read every open project: the backstop under the watch.
    ///
    /// Coming back to the window is where a missed event costs the most, and
    /// it is the one moment we can be sure of catching. A file moved in from
    /// outside the tree, a network mount the platform reports nothing for, an
    /// event dropped while the queue overflowed — none of those reach the
    /// watch, and all of them are corrected here.
    pub fn reload_projects(&mut self, cx: &mut Context<Self>) {
        let mut moved = false;
        for ix in 0..self.projects.len() {
            moved |= self.projects[ix].reload(cx);
        }
        if moved {
            cx.emit(Reloaded);
        }
        cx.notify();
    }
}
