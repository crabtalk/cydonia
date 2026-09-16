//! The agent sessions running in a project, and what a turn does to one.
//!
//! A continuation of [`Workspace`]'s one `impl`, which is why it opens on
//! `use super::*`: these methods work on the same struct and reach the same
//! names as the rest of it.
use super::*;

impl Workspace {
    /// Open a session in the active project. `seed` is its first prompt, sent
    /// as soon as the agent is up — what a dispatched card rides in on.
    ///
    /// The one place a session is born, so it is where the sessions switch
    /// bites: nothing spawns an agent until it has been turned on.
    pub fn new_session(
        &mut self,
        entry: settings::Agent,
        seed: Option<String>,
        cx: &mut Context<Self>,
    ) -> Option<u64> {
        if !self.settings.features.sessions {
            return None;
        }
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

    pub fn retain_panel_session(
        &mut self,
        id: u64,
        remember: bool,
        cx: &mut Context<Self>,
    ) -> Option<(PathBuf, String)> {
        let ix = self.project_of(id)?;
        let record = self.projects[ix].session_mut(id)?.retain_panel()?;
        let cwd = self.projects[ix].path.clone();
        if remember {
            self.remember(ix, state::Kind::Session, record.clone());
        }
        cx.notify();
        Some((cwd, record))
    }

    pub fn fork_session(
        &mut self,
        source: u64,
        before: usize,
        cx: &mut Context<Self>,
    ) -> Option<u64> {
        if !self.settings.features.sessions {
            return None;
        }
        let ix = self.project_of(source)?;
        self.projects[ix].session_mut(source)?.mint_record()?;
        let id = self.next_id;
        let mut fork = self.projects[ix].session(source)?.fork_at(id, before)?;
        self.next_id += 1;
        fork.flush();
        self.projects[ix].sessions.push(fork);
        self.select_session(id, cx);
        Some(id)
    }

    pub fn set_draft(&mut self, id: u64, draft: String, cx: &mut Context<Self>) {
        if self.session(id).is_none_or(|chat| chat.draft == draft) {
            return;
        }
        // Coalesce typing so a large transcript is not rewritten on every keystroke.
        let save = cx.spawn(async move |workspace, cx| {
            cx.background_executor()
                .timer(std::time::Duration::from_millis(500))
                .await;
            let _ = workspace.update(cx, |workspace, cx| {
                workspace.with_session(id, cx, |chat| {
                    chat.flush();
                    chat.draft_save = None;
                });
            });
        });
        self.with_session(id, cx, |chat| {
            chat.draft = draft;
            chat.draft_save = Some(save);
        });
    }

    /// Every project's sessions are on show, so picking one brings its project
    /// forward with it.
    pub fn select_session(&mut self, id: u64, cx: &mut Context<Self>) {
        let Some(ix) = self.project_of(id) else {
            return;
        };
        self.projects[ix].active = Some(id);
        self.active = Some(ix);
        // A session has no file until its first turn is written, so one that
        // has said nothing is not yet somewhere to come back to.
        if let Some(record) = self.projects[ix]
            .session(id)
            .and_then(|chat| chat.record.clone())
        {
            self.remember(ix, state::Kind::Session, record);
        }
        self.wake_session(id, cx);
        cx.notify();
    }

    /// Point a session at an agent, if it has none and could have one.
    ///
    /// Called where a session is brought *forward* rather than where one is
    /// created: the session you are looking at is the one you are about to work
    /// in, and what an agent reports on connect — its modes, its model — is
    /// what the composer needs before the first prompt rather than after it.
    ///
    /// Only ever the one in front. Every other session a project holds stays
    /// idle, which is what still keeps a launch from starting an agent per
    /// transcript. An archived one stays where it was put.
    pub(super) fn wake_session(&mut self, id: u64, cx: &mut Context<Self>) {
        if !self.settings.features.sessions {
            return;
        }
        let found = self
            .projects
            .iter_mut()
            .find_map(|project| project.session_mut(id))
            .filter(|chat| chat.idle() && chat.resumable() && !chat.closed);
        if let Some(chat) = found {
            chat.resume(cx);
            cx.notify();
        }
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
    pub(super) fn restore_sessions(&mut self, ix: usize) {
        let path = self.projects[ix].path.clone();
        for stored in self.projects[ix].store().sessions() {
            let id = self.next_id;
            self.next_id += 1;
            let entry = super::named(
                &self.settings.agents,
                stored.agent_id.as_deref(),
                &stored.agent,
            )
            .cloned()
            // Nothing on this machine answers to it. The placeholder is a
            // name to show and no command to start, which is what
            // `Workspace::reachable` reads as stranded — and it keeps the
            // id, so the record is written back as findable as it arrived.
            .unwrap_or_else(|| settings::Agent {
                name: stored.agent.clone(),
                id: stored.agent_id.clone(),
                command: String::new(),
                args: Vec::new(),
                env: Default::default(),
            });
            let chat = ChatSession::restore(id, path.clone(), entry, stored);
            self.projects[ix].sessions.push(chat);
        }
    }

    /// Send to a session, starting an agent for it when it has none — typing
    /// into a session read back from disk is what picks it up again.
    pub fn send(&mut self, id: u64, content: String, cx: &mut Context<Self>) {
        // Read before `chat` borrows the projects. This is the second place an
        // agent process starts, so it is the second half of the gate.
        let enabled = self.settings.features.sessions;
        let found = self
            .projects
            .iter_mut()
            .find_map(|project| project.session_mut(id));
        let Some(chat) = found else {
            return;
        };
        // Said out loud rather than queued: with no agent to drain it, a
        // prompt pushed onto the queue reads as a message that went nowhere.
        if !enabled && !chat.live() {
            chat.notice(
                true,
                "sessions are off — turn them on in Settings › Features",
            );
            cx.notify();
            return;
        }
        // Sending explicitly brings an archived conversation back into use.
        chat.closed = false;
        if chat.idle() && chat.resumable() {
            chat.resume(cx);
        }
        chat.send(content);
        let record = chat.record.clone();
        if let (Some(record), Some(ix)) = (record, self.project_of(id)) {
            self.remember(ix, state::Kind::Session, record);
        }
        cx.notify();
    }

    /// Send a message with pictures. Each is kept in the project's assets and
    /// written into the message as a line of its own, which is what the
    /// transcript paints and what the prompt reads back out for the agent.
    pub fn send_attached(
        &mut self,
        id: u64,
        text: String,
        attachments: &[crate::model::media::Attachment],
        cx: &mut Context<Self>,
    ) {
        use crate::model::media;
        let Some(ix) = self.project_of(id) else {
            return;
        };
        let project = artifact::project::fs::Project::new(&self.projects[ix].path);
        let mut parts = vec![text.trim_end().to_owned()];
        if project.init().is_ok() {
            let dir = project.assets();
            parts.extend(
                attachments
                    .iter()
                    .filter_map(|attachment| media::keep_attachment(&dir, attachment))
                    .map(|path| media::line(&path)),
            );
        }
        parts.retain(|part| !part.is_empty());
        if !parts.is_empty() {
            self.send(id, parts.join("\n\n"), cx);
        }
    }

    /// Close the connection and keep the transcript. The row stays where it
    /// was, readable, and typing into it opens an agent again.
    /// Put a session away, or bring it back. Closing tears the agent down and
    /// keeps the transcript; opening it again is what reconnects.
    pub fn archive_session(&mut self, id: u64, archived: bool, cx: &mut Context<Self>) {
        self.with_session(id, cx, |chat| match archived {
            true => chat.close(),
            false => {
                chat.closed = false;
                chat.flush();
            }
        });
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
        if let Some(record) = project.session(id).and_then(|chat| chat.record.clone()) {
            project.store().remove_session(&record);
        }
        project.sessions.retain(|chat| chat.id != id);
        if project.active == Some(id) {
            // The view chooses the next entry in sidebar order, across all
            // kinds. Storage order can put an archived session first.
            project.active = None;
        }
        cx.notify();
    }

    /// Any session, in whichever project holds it — the pump that feeds a
    /// session knows only its id, and must not care which tab it sits behind.
    pub fn session(&self, id: u64) -> Option<&ChatSession> {
        self.projects.iter().find_map(|project| project.session(id))
    }

    /// The session filed under this id, wherever it is — how a card finds the
    /// agent it was handed to after a launch that renumbered every session.
    /// [`ChatSession::id`] is minted per launch and means nothing on disk;
    /// this is the name that keeps.
    pub fn session_by_record(&self, record: &str) -> Option<&ChatSession> {
        self.projects
            .iter()
            .flat_map(|project| project.sessions.iter())
            .find(|chat| chat.record.as_deref() == Some(record))
    }

    /// The id a session is filed under, minted if it has none — what a
    /// dispatched card writes down.
    pub fn mint_record(&mut self, id: u64) -> Option<String> {
        self.projects
            .iter_mut()
            .find_map(|project| project.session_mut(id))?
            .mint_record()
            .map(str::to_owned)
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

    /// Put what the transcript has selected on the clipboard — see
    /// [`crate::view::component::transcript::State::copied`].
    ///
    /// Answers whether there was anything, so a `cmd-c` that finds no selection
    /// can be left to whatever else wanted it.
    pub fn copy_selection(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(text) = self
            .active_session()
            .and_then(|chat| chat.transcript.copied(chat))
        else {
            return false;
        };
        cx.write_to_clipboard(ClipboardItem::new_string(text));
        true
    }

    /// Switch a session's mode — what the composer's mode picker reports.
    /// See [`ChatSession::set_mode`].
    pub fn set_session_mode(&mut self, id: u64, mode_id: String, cx: &mut Context<Self>) {
        self.with_session(id, cx, |chat| chat.set_mode(&mode_id));
    }

    /// The same for a config option, which is where the model lives.
    pub fn set_session_config(
        &mut self,
        id: u64,
        config_id: String,
        value: SessionConfigOptionValue,
        cx: &mut Context<Self>,
    ) {
        self.with_session(id, cx, |chat| chat.set_config(&config_id, value));
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

    /// The session the chat pane would show. Gated, and it is the gate that
    /// matters most: sessions are read back off disk when a project opens,
    /// whatever the switch says, so without this a filed transcript would put
    /// the pane on screen with no composer under it.
    pub fn active_session(&self) -> Option<&ChatSession> {
        self.settings
            .features
            .sessions
            .then(|| self.active_project()?.active_session())
            .flatten()
    }

    pub fn active_id(&self) -> Option<u64> {
        self.active_project().and_then(|project| project.active)
    }

    /// Whether the session in front has anywhere to send: it is talking to an
    /// agent already, or `settings.toml` still names the one it would start.
    ///
    /// Stronger than [`ChatSession::resumable`], which asks only whether the
    /// session was handed a command. A session outlives the agent it was
    /// opened on — uninstalled in Settings › Agents, or edited out of the
    /// file — and the copy of the entry it is holding says nothing about
    /// whether that command is still on this machine. What is left reads back
    /// and cannot be answered into, which is what the composer's absence has
    /// to mean.
    ///
    /// A live session is reachable whatever the file says. It is running: the
    /// agent was uninstalled out from under a conversation, and taking the
    /// composer away mid-turn would strand it.
    pub fn reachable(&self) -> bool {
        let Some(chat) = self.active_session() else {
            return false;
        };
        chat.live() || (chat.resumable() && self.agent_for(&chat.entry).is_some())
    }
}
