//! The projects on the rail: opening, ordering, closing, and the watch
//! each one carries.
//!
//! A continuation of [`Workspace`]'s one `impl`, which is why it opens on
//! `use super::*`: these methods work on the same struct and reach the same
//! names as the rest of it.
use super::*;
use artifact::kind::Kind;

impl Workspace {
    /// The projects on the rail, in the order the sidebar lists them, then
    /// the ones still opening.
    pub(super) fn locations(&self) -> Vec<Location> {
        self.projects
            .iter()
            .map(Project::location)
            .chain(self.opening.iter().map(|opening| &opening.location))
            .cloned()
            .collect()
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
        rail::install_stores(store::open);
        rail::set_open(self.locations());
        rail::set_agents(self.rail_agents());
        cx.spawn(async move |workspace, cx| {
            while let Some(change) = asks.next().await {
                let held = workspace
                    .update(cx, |workspace, cx| match change {
                        Change::Open(location) => workspace.open_project_at(location, cx),
                        Change::Close(location) => workspace.close_project_at(&location, cx),
                        Change::Send { session, message } => {
                            workspace.send_to_record(&session, message, cx)
                        }
                        Change::Rename { session, title } => {
                            workspace.retitle_record(&session, title, cx)
                        }
                        Change::Label { session, labels } => {
                            workspace.label_record(&session, labels, cx)
                        }
                        Change::Start {
                            project,
                            agent,
                            message,
                        } => workspace.start_in(&project, &agent, message, cx),
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
    ///
    /// A project on another machine is opened on its host first, and is put
    /// on the rail and brought forward once it lands.
    pub fn open_project(&mut self, location: Location, cx: &mut Context<Self>) {
        if let Some(ix) = self.projects.iter().position(|p| *p.location() == location) {
            self.select_project(ix, cx);
            return;
        }
        let Some(store) = store::open(&location) else {
            #[cfg(feature = "desktop")]
            self.open_remote(location, true, true, cx);
            return;
        };
        let ix = self.hold_project(location, store, true, cx);
        self.select_project(ix, cx);
    }

    /// Put the project at `location`, kept in `store`, at the end of the rail
    /// with what it has on disk, and answer where it is.
    pub(super) fn hold_project(
        &mut self,
        location: Location,
        store: Store,
        expanded: bool,
        cx: &mut Context<Self>,
    ) -> usize {
        let mut project = Project::new(location, store, cx);
        project.expanded = expanded;
        self.projects.push(project);
        let ix = self.projects.len() - 1;
        self.list_tables(ix, cx);
        self.restore_sessions(ix, cx);
        self.watch_project(ix, cx);
        ix
    }

    /// Open the project at `location` on its host, holding it as opening —
    /// listed in `state.toml`, not on the rail — until it lands. Landed, it is
    /// put on the rail, and brought forward with `select`. One closed while
    /// opening is let go.
    #[cfg(feature = "desktop")]
    pub(super) fn open_remote(
        &mut self,
        location: Location,
        expanded: bool,
        select: bool,
        cx: &mut Context<Self>,
    ) {
        use crate::model::remote::Remotes;
        if self
            .opening
            .iter()
            .any(|opening| opening.location == location)
        {
            return;
        }
        self.opening.push(Opening {
            location: location.clone(),
            expanded,
        });
        self.save();
        cx.spawn(async move |workspace, cx| {
            let opened = match Remotes::get() {
                Ok(remotes) => remotes.open(&location).await,
                Err(error) => Err(error),
            };
            // TODO(DEV-324): a project that does not land stays opening, with
            // nothing on screen saying why.
            let Ok(project) = opened else {
                return;
            };
            let _ = workspace.update(cx, |workspace, cx| {
                let Some(at) = workspace
                    .opening
                    .iter()
                    .position(|opening| opening.location == location)
                else {
                    if let Ok(remotes) = Remotes::get() {
                        remotes.close(&location);
                    }
                    return;
                };
                let opening = workspace.opening.remove(at);
                let ix =
                    workspace.hold_project(location, Store::Remote(project), opening.expanded, cx);
                match select {
                    true => workspace.select_project(ix, cx),
                    false => {
                        workspace.save();
                        cx.notify();
                    }
                }
            });
        })
        .detach();
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

    /// Show what the project has archived under its divider, or fold it away.
    pub fn toggle_archive(&mut self, ix: usize, cx: &mut Context<Self>) {
        if let Some(open) = self.projects.get_mut(ix) {
            open.archive_open = !open.archive_open;
            cx.notify();
        }
    }

    /// Carry a project to another place in the list. `active` follows the
    /// project it points at rather than the index it sits on: which one is in
    /// front has nothing to do with what order they are listed in.
    pub fn move_project(&mut self, location: &Location, to: usize, cx: &mut Context<Self>) {
        let Some(from) = self.project_at(location) else {
            return;
        };
        if from == to || to >= self.projects.len() {
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

    /// Whichever project is at `location`, by the location the sidebar holds
    /// it under or by what that settles to — see [`Location::settled`].
    ///
    /// The tools settle a path before handing it over and the directory picker
    /// does not — see `mcp::tools::project` — so `/tmp/x` on the rail and
    /// `/private/tmp/x` from a tool are one project, and opening the second
    /// would otherwise list the same directory twice.
    pub fn project_at(&self, location: &Location) -> Option<usize> {
        if let Some(ix) = self
            .projects
            .iter()
            .position(|open| open.location() == location)
        {
            return Some(ix);
        }
        let settled = location.settled();
        self.projects
            .iter()
            .position(|open| open.location().settled() == settled)
    }

    /// Bring the project at `location` forward, opening it where it is not on
    /// the rail at all. What the tools ask for; the sidebar names a row
    /// instead.
    fn open_project_at(&mut self, location: Location, cx: &mut Context<Self>) {
        match self.project_at(&location) {
            Some(ix) => self.select_project(ix, cx),
            None => self.open_project(location, cx),
        }
    }

    /// Close whichever project is at `location`, if one is, or stop opening
    /// it.
    fn close_project_at(&mut self, location: &Location, cx: &mut Context<Self>) {
        let before = self.opening.len();
        self.opening.retain(|opening| opening.location != *location);
        if self.opening.len() != before {
            self.save();
        }
        let Some(ix) = self.project_at(location) else {
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
        let closed = self.projects.remove(ix);
        #[cfg(feature = "desktop")]
        if closed.location().host().is_some()
            && let Ok(remotes) = crate::model::remote::Remotes::get()
        {
            remotes.close(closed.location());
        }
        drop(closed);
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
            .and_then(|open| self.last.get(open.location()))
        else {
            return;
        };
        let (kind, id) = (entry.kind, entry.id.clone());
        let Some(project) = self.projects.get_mut(ix) else {
            return;
        };
        let showing = project.stored(kind, &id, cx);
        let at = showing.as_ref().and_then(|showing| project.ix_of(showing));
        match kind {
            Kind::Session => {
                project.active = match showing {
                    Some(Showing::Session(id)) => Some(id),
                    _ => None,
                };
            }
            Kind::Board => project.board = at,
            Kind::Article => {
                project.article = at;
                if let Some(at) = project.article {
                    project.articles[at].open(cx);
                }
            }
            Kind::Table => {
                project.table = at;
                project.reload_page(cx);
            }
            Kind::App => {}
        }
        // Landing back in a session is being in front of it — see
        // [`Self::wake_session`]. Landing in an article or a board is not, and
        // starts nothing.
        let woken = self
            .projects
            .get(ix)
            .filter(|_| matches!(kind, Kind::Session))
            .and_then(|open| open.active);
        if let Some(id) = woken {
            self.wake_session(id, cx);
        }
        cx.notify();
    }

    /// What the active project was last showing, by kind. What the window puts
    /// its pane on when it lands here — [`Self::open_last_entry`] opens the
    /// entry, and this is how the view learns which one of the four it was.
    pub fn landing(&self) -> Option<Kind> {
        let open = self.active_project()?;
        self.last.get(open.location()).map(|entry| entry.kind)
    }

    /// The active project, and the entry it was last showing.
    pub fn landed(&self) -> Option<(&Location, &state::Entry)> {
        let open = self.active_project()?;
        Some((open.location(), self.last.get(open.location())?))
    }

    /// Remember the entry a project is now showing, so the next launch lands on
    /// it. Every way of opening one arrives here.
    pub(super) fn remember(
        &mut self,
        project: usize,
        kind: Kind,
        id: String,
        cx: &mut Context<Self>,
    ) {
        // Opening any entry leaves the arrangement. Here rather than in each
        // `open_*`: this is the one place they all come through, so there is
        // no way to open an entry and forget to.
        self.leave_space();
        self.note_landing(project, kind, id, cx);
    }

    /// The same, for something that touched an entry without being asked to
    /// open it — sending into a session is the case, and it must not take the
    /// window out of the arrangement the session is a pane of.
    pub(super) fn note_landing(
        &mut self,
        project: usize,
        kind: Kind,
        id: String,
        cx: &mut Context<Self>,
    ) {
        let Some(open) = self.projects.get(project) else {
            return;
        };
        self.last
            .insert(open.location().clone(), state::Entry { kind, id });
        self.prune_archived(cx);
        self.save();
    }

    /// Every archive passes through here, an agent's read back off disk
    /// included.
    pub(super) fn prune_archived(&mut self, cx: &mut Context<Self>) {
        self.prune_archived_for(self.landing(), cx);
        self.drop_archived_from_spaces(cx);
    }

    pub(super) fn prune_archived_for(&mut self, kind: Option<Kind>, cx: &mut Context<Self>) {
        for (ix, project) in self.projects.iter_mut().enumerate() {
            let active = self.active == Some(ix);
            for held in &project.sessions {
                if !(active && kind == Some(Kind::Session) && project.active == Some(held.id)) {
                    held.chat.update(cx, |chat, _| chat.unload_history());
                }
            }
            for (at, article) in project.articles.iter_mut().enumerate() {
                if !(active && kind == Some(Kind::Article) && project.article == Some(at)) {
                    article.unload(cx);
                }
            }
            if !(active && kind == Some(Kind::Table))
                && project
                    .table
                    .and_then(|at| project.tables.get(at))
                    .is_some_and(|table| table.archived)
            {
                project.pages.clear();
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
        let Some(open) = self.projects.get(ix) else {
            return;
        };
        let watch = Watch::open(open.location().clone(), open.store(), cx);
        if let Some(project) = self.projects.get_mut(ix) {
            project.watch = Some(watch);
        }
    }

    /// Re-read one project off disk and reconcile it. Addressed by location
    /// rather than by index because the watch that calls this outlives any
    /// index it could have been armed with — the rail is reorderable, and
    /// closing a project shifts every one after it.
    pub fn reload_project(&mut self, location: &Location, cx: &mut Context<Self>) {
        let Some(ix) = self
            .projects
            .iter()
            .position(|open| open.location() == location)
        else {
            return;
        };
        if self.projects[ix].reload(cx) {
            cx.emit(Reloaded);
        }
        let tables = self.projects[ix].reload_tables(cx);
        self.prune_archived(cx);
        // An entry deleted from under a space leaves a member naming a
        // number nothing answers to.
        self.prune_spaces(cx);
        cx.notify();
        // A table archived or dropped is known only once the tables land.
        cx.spawn(async move |workspace, cx| {
            let moved = tables.await;
            let _ = workspace.update(cx, |workspace, cx| {
                if moved {
                    cx.emit(Reloaded);
                }
                workspace.prune_archived(cx);
                workspace.prune_spaces(cx);
            });
        })
        .detach();
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
        let mut tables = Vec::new();
        for ix in 0..self.projects.len() {
            moved |= self.projects[ix].reload(cx);
            tables.push(self.projects[ix].reload_tables(cx));
        }
        if moved {
            cx.emit(Reloaded);
        }
        cx.notify();
        cx.spawn(async move |workspace, cx| {
            if futures::future::join_all(tables).await.contains(&true) {
                let _ = workspace.update(cx, |_, cx| cx.emit(Reloaded));
            }
        })
        .detach();
    }
}
