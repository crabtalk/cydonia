//! The agent sessions running in a project, and what a turn does to one.
//!
//! A continuation of [`Workspace`]'s one `impl`, which is why it opens on
//! `use super::*`: these methods work on the same struct and reach the same
//! names as the rest of it.
use super::*;
use crate::model::{
    project::HeldSession,
    session_preferences::{self, OfferCache},
    workspace::Member,
};
use artifact::{kind::Kind, project::Project as _};
use bezel::gpui::Entity;

/// One pick on an article's composer before its session exists, which changes
/// the article's default. The agent is its index in `settings.agents`.
#[derive(Clone)]
pub enum ArticleChange {
    Agent(usize),
    Mode(String),
    Config { id: String, value: String },
}

impl Workspace {
    /// Open a session in the active project. `seed` is its first prompt, sent
    /// as soon as the agent is up — what a card sent to a new session rides in on.
    ///
    /// The one place a session is born, so it is where the sessions switch
    /// bites: nothing spawns an agent until it has been turned on.
    pub fn new_session(
        &mut self,
        entry: settings::Agent,
        seed: Option<String>,
        cx: &mut Context<Self>,
    ) -> Option<u64> {
        let ix = self.active?;
        self.new_session_in(ix, entry, seed, true, cx)
    }

    /// The link to turns `from..=to` of a session, from 1, filing the session
    /// first so it has the number the link names.
    pub fn turn_link(&mut self, id: u64, from: usize, to: usize, cx: &mut App) -> Option<String> {
        self.mint_record(id, cx)?;
        self.session(id, cx)?.turn_link(from, to)
    }

    /// How a session is written as a reference, `project#43`: its project's
    /// directory name and its entry number. Nothing before it has a number.
    pub fn link_of_session(&self, id: u64, cx: &App) -> Option<String> {
        let project = &self.projects[self.project_of(id)?];
        let number = project.session(id, cx)?.number?;
        let name = artifact::reference::project_name(&project.path)?;
        Some(artifact::reference::Reference::entry(Some(&name), number, None).link())
    }

    /// [`Self::new_session`] in the project at `ix` rather than the active one.
    /// `front` puts it in front of its project; without it, whatever the
    /// window shows stays put.
    pub fn new_session_in(
        &mut self,
        ix: usize,
        entry: settings::Agent,
        seed: Option<String>,
        front: bool,
        cx: &mut Context<Self>,
    ) -> Option<u64> {
        self.start_session(ix, front, cx, |id, cwd, cx| {
            ChatSession::connect(id, entry, cwd, seed, cx)
        })
    }

    /// Put the session `make` opens, given its id and its project's directory,
    /// in the project at `ix`. `front` puts it in front of its project.
    pub(super) fn start_session(
        &mut self,
        ix: usize,
        front: bool,
        cx: &mut Context<Self>,
        make: impl FnOnce(u64, PathBuf, &mut Context<ChatSession>) -> ChatSession,
    ) -> Option<u64> {
        if !self.settings.features.sessions || ix >= self.projects.len() {
            return None;
        }
        let id = self.next_id;
        self.next_id += 1;
        let cwd = self.projects[ix].path.clone();
        let chat = cx.new(|cx| make(id, cwd, cx));
        let project = &mut self.projects[ix];
        project.sessions.push(HeldSession { id, chat });
        if front {
            project.active = Some(id);
        }
        self.prune_archived_for(Some(Kind::Session), cx);
        cx.notify();
        Some(id)
    }

    /// The member that names a session wherever it is open — what finds the
    /// tab holding it. Nothing for one with no record id yet.
    pub fn member_of_session(&self, id: u64, cx: &App) -> Option<Member> {
        self.member_of(self.project_of(id)?, super::Showing::Session(id), cx)
    }

    /// Give a session the name that keeps, and write its file — what anything
    /// pointing at a session from outside the process needs.
    pub fn retain_session(&mut self, id: u64, cx: &mut Context<Self>) -> Option<String> {
        let record = self.update_session(id, cx, ChatSession::retain_panel)??;
        cx.notify();
        Some(record)
    }

    /// Keep the session in front on disk, and remember it as its project's
    /// last entry.
    ///
    /// Called from [`Cydonia::save_panel_layout`], which is the only thing
    /// that runs often enough to catch a quit: nothing else writes a session's
    /// record between the one that opened it and the one that closes it.
    pub fn retain_active_session(&mut self, id: u64, cx: &mut Context<Self>) -> Option<String> {
        let ix = self.project_of(id)?;
        let record = self.retain_session(id, cx)?;
        self.note_landing(ix, Kind::Session, record.clone(), cx);
        cx.notify();
        Some(record)
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
        self.mint_record(source, cx)?;
        let id = self.next_id;
        let mut fork = self.session(source, cx)?.fork_at(id, before)?;
        self.next_id += 1;
        fork.flush();
        let fork = HeldSession::new(fork, cx);
        self.projects[ix].sessions.push(fork);
        self.select_session(id, cx);
        Some(id)
    }

    /// See [`ChatSession::rewind`].
    pub fn rewind_session(&mut self, id: u64, at: usize, cx: &mut Context<Self>) -> Option<String> {
        let chat = self.session_entity(id)?.clone();
        let text = chat.update(cx, |chat, cx| {
            let text = chat.rewind(at, cx);
            cx.notify();
            text
        })?;
        Some(text)
    }

    pub fn set_draft(&mut self, id: u64, draft: String, cx: &mut Context<Self>) {
        if !self.load_history(id, cx) {
            return;
        }
        if self.session(id, cx).is_none_or(|chat| chat.draft == draft) {
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
                workspace.prune_archived(cx);
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
        if !self.load_history(id, cx) {
            return;
        }
        self.projects[ix].active = Some(id);
        self.active = Some(ix);
        // A session with no file yet is not somewhere to come back to.
        if let Some(record) = self.projects[ix]
            .session(id, cx)
            .and_then(|chat| chat.filed().map(str::to_owned))
        {
            self.note_landing(ix, Kind::Session, record, cx);
        }
        self.wake_session(id, cx);
        self.prune_archived_for(Some(Kind::Session), cx);
        cx.notify();
    }

    /// Point a session at an agent, if it has none and could have one.
    ///
    /// Called where a session is brought *forward* rather than where one is
    /// created: the session you are looking at is the one you are about to work
    /// in, and what an agent reports on connect — its modes, its model — is
    /// what the composer needs before the first prompt rather than after it.
    ///
    /// Only the one in front, or one a composer asks to reconnect. Every other
    /// session a project holds stays idle, which is what still keeps a launch
    /// from starting an agent per transcript. An archived one stays where it
    /// was put.
    pub(crate) fn wake_session(&mut self, id: u64, cx: &mut Context<Self>) {
        if !self.settings.features.sessions {
            return;
        }
        let Some(chat) = self.session_entity(id).cloned() else {
            return;
        };
        chat.update(cx, |chat, cx| {
            if chat.idle() && chat.resumable() && !chat.closed {
                chat.resume(cx);
                cx.notify();
            }
        });
    }

    /// Where the open project holding session `id` is listed.
    pub fn project_of(&self, id: u64) -> Option<usize> {
        self.projects
            .iter()
            .position(|project| project.session_entity(id).is_some())
    }

    /// Read the project's filed sessions back, minting an id for each — ids
    /// mean nothing across a launch, so a reloaded one is as new as any. The
    /// agent is resolved by name; a session whose agent has since left
    /// `settings.toml` comes back readable but cannot reconnect.
    pub(super) fn restore_sessions(&mut self, ix: usize, cx: &mut App) {
        let path = self.projects[ix].path.clone();
        let records = self.projects[ix].store().sessions();
        let _ = session_preferences::keep_sessions(&path, records.iter().map(|r| r.id.as_str()));
        for stored in records {
            self.restore_record(ix, stored, cx);
        }
    }

    /// Hold one filed session in the project at `ix`, idle, its transcript
    /// left on disk.
    pub(super) fn restore_record(
        &mut self,
        ix: usize,
        stored: artifact::session::record::Record,
        cx: &mut App,
    ) {
        let path = self.projects[ix].path.clone();
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
        let identity = stored.identity.clone();
        let mut chat = ChatSession::restore(id, path.clone(), entry, stored);
        if let Some(identity) = identity.as_deref().and_then(|id| self.identity(id)) {
            chat.adopt(identity);
        }
        chat.unload_history();
        let chat = HeldSession::new(chat, cx);
        self.projects[ix].sessions.push(chat);
    }

    /// Send to a session, starting an agent for it when it has none — typing
    /// into a session read back from disk is what picks it up again.
    pub fn send(&mut self, id: u64, content: String, cx: &mut Context<Self>) {
        if !self.load_history(id, cx) {
            return;
        }
        // This is the second place an agent process starts, so it is the
        // second half of the gate.
        let enabled = self.settings.features.sessions;
        let Some(chat) = self.session_entity(id).cloned() else {
            return;
        };
        let record = chat.update(cx, |chat, cx| {
            cx.notify();
            // Said out loud rather than queued: with no agent to drain it, a
            // prompt pushed onto the queue reads as a message that went
            // nowhere.
            if !enabled && !chat.live() {
                chat.notice(
                    true,
                    "sessions are off — turn them on in Settings › Features",
                );
                return None;
            }
            // Sending explicitly brings an archived conversation back into use.
            chat.closed = false;
            if chat.idle() && chat.resumable() {
                chat.resume(cx);
            }
            // A message opening with a command the agent offers sends it.
            let command = content
                .strip_prefix('/')
                .and_then(|rest| rest.split_whitespace().next())
                .filter(|name| chat.commands.iter().any(|command| command.name == *name))
                .map(|name| (chat.entry.name.clone(), name.to_owned()));
            chat.send(content);
            Some((chat.filed().map(str::to_owned), command))
        });
        let Some((record, command)) = record else {
            return;
        };
        if let Some((agent, command)) = command {
            self.sent_command(agent, command);
        }
        if let Some(minted) = self.session(id, cx).and_then(|chat| chat.record.clone()) {
            self.keep_edited(Kind::Session, &minted);
        }
        if let (Some(record), Some(ix)) = (record, self.project_of(id)) {
            self.note_landing(ix, Kind::Session, record, cx);
        }
        cx.notify();
    }

    /// The session the article at `path` shows: the newest opened under it, or
    /// a new one on the preferred agent. Nothing while article threads are
    /// off.
    pub fn article_session(&mut self, path: &Path, cx: &mut Context<Self>) -> Option<u64> {
        self.article_thread(path, cx)
            .or_else(|| self.new_article_session(path, cx))
    }

    /// The newest session opened under the article at `path`, opening none.
    /// Nothing while article threads are off.
    pub fn article_thread(&self, path: &Path, cx: &App) -> Option<u64> {
        let (ix, article) = self.thread_article(path)?;
        self.projects[ix]
            .chats(cx)
            .filter(|chat| chat.article.as_deref() == Some(article.as_str()))
            .max_by_key(|chat| chat.touched())
            .map(|chat| chat.id)
    }

    /// Open another session under the article at `path` on the article's
    /// default agent, which the article shows from then on — see
    /// [`Self::article_thread`].
    pub fn new_article_session(&mut self, path: &Path, cx: &mut Context<Self>) -> Option<u64> {
        let (ix, article) = self.thread_article(path)?;
        let entry = self.article_agent(cx)?;
        let choices = self.article_choices(&entry);
        let id = self.next_id;
        self.next_id += 1;
        let cwd = self.projects[ix].path.clone();
        let chat = cx.new(|cx| ChatSession::connect_article(id, entry, cwd, article, choices, cx));
        self.projects[ix].sessions.push(HeldSession { id, chat });
        cx.notify();
        Some(id)
    }

    /// The agent an article's session opens on: the article default's, while
    /// it is still configured, and otherwise [`Self::preferred_agent`].
    pub fn article_agent(&self, cx: &App) -> Option<settings::Agent> {
        let default = self.settings.article_session.agent.as_deref();
        self.settings
            .agents
            .iter()
            .find(|agent| default == Some(session_preferences::agent_key(agent).as_str()))
            .cloned()
            .or_else(|| self.preferred_agent(cx))
    }

    /// The choices a session opened on `agent` starts with: the article
    /// default's, when the default is for that agent.
    fn article_choices(&self, agent: &settings::Agent) -> session_preferences::Choices {
        let article = &self.settings.article_session;
        if article.agent.as_deref() == Some(session_preferences::agent_key(agent).as_str()) {
            article.choices()
        } else {
            session_preferences::Choices::default()
        }
    }

    /// What an article's session would open on before one is opened: its
    /// agent, and that agent's offers in the article's project with the
    /// default's choices on.
    pub fn article_offer(
        &self,
        path: &Path,
        cx: &App,
    ) -> Option<(settings::Agent, session_preferences::Offers)> {
        let (ix, _) = self.thread_article(path)?;
        self.article_default_offers(&self.projects[ix].path, cx)
    }

    /// The same for the project at `cwd`, which the Settings screen reads the
    /// article default's options from.
    pub fn article_default_offers(
        &self,
        cwd: &Path,
        cx: &App,
    ) -> Option<(settings::Agent, session_preferences::Offers)> {
        let agent = self.article_agent(cx)?;
        let choices = self.article_choices(&agent);
        let offers = OfferCache::get(cx, cwd, &agent).with(&choices);
        Some((agent, offers))
    }

    /// Change the article default by one pick made on the article default's
    /// own agent — see [`Self::remember_article`].
    pub fn set_article_default(
        &mut self,
        cwd: &Path,
        change: ArticleChange,
        cx: &mut Context<Self>,
    ) {
        let Some(current) = self.article_agent(cx) else {
            return;
        };
        self.remember_article(cwd, &current, change, cx);
    }

    /// Change the article default by one pick made on `current`, with the
    /// offers read from the project at `cwd`. A pick the agent does not offer
    /// is dropped. A pick on another agent than the default's, or a change of
    /// agent, starts the choices over on that agent.
    fn remember_article(
        &mut self,
        cwd: &Path,
        current: &settings::Agent,
        change: ArticleChange,
        cx: &mut Context<Self>,
    ) {
        let key = match &change {
            ArticleChange::Agent(ix) => match self.settings.agents.get(*ix) {
                Some(entry) => session_preferences::agent_key(entry),
                None => return,
            },
            _ => session_preferences::agent_key(current),
        };
        let mut next = self.settings.article_session.clone();
        if next.agent.as_deref() != Some(key.as_str()) {
            next = settings::ArticleSession {
                agent: Some(key),
                ..Default::default()
            };
        }
        let offers = OfferCache::get(cx, cwd, current);
        match change {
            ArticleChange::Agent(_) => {}
            ArticleChange::Mode(mode) => {
                if offers.modes.is_some_and(|modes| {
                    modes
                        .available_modes
                        .iter()
                        .any(|m| &*m.id == mode.as_str())
                }) {
                    next.mode = Some(mode);
                }
            }
            ArticleChange::Config { id, value } => {
                let choice = cydonia_acp::proto::SessionConfigOptionValue::ValueId {
                    value: value.as_str().into(),
                };
                let offered = offers.config.iter().any(|option| {
                    &*option.id == id.as_str() && session_preferences::supports(option, &choice)
                });
                if offered {
                    next.config.insert(id, value);
                }
            }
        }
        let _ = settings::set_article_session(&next);
        self.settings.article_session = next;
        cx.notify();
    }

    /// Whether the composer under the article at `path` has anywhere to send:
    /// its session is [`Self::reachable_session`], or it has none yet and
    /// there is an agent to open one on.
    pub fn thread_reachable(&self, path: &Path, cx: &App) -> bool {
        if self.thread_article(path).is_none() {
            return false;
        }
        match self
            .article_thread(path, cx)
            .and_then(|id| self.session(id, cx))
        {
            Some(chat) => self.reachable_session(chat),
            None => self.preferred_agent(cx).is_some(),
        }
    }

    /// The directory of the project holding the article at `path`, while
    /// article threads are on.
    pub fn article_cwd(&self, path: &Path) -> Option<PathBuf> {
        let (ix, _) = self.thread_article(path)?;
        Some(self.projects[ix].path.clone())
    }

    /// The project holding the article at `path`, and the article's id, while
    /// article threads are on.
    fn thread_article(&self, path: &Path) -> Option<(usize, String)> {
        if !self.settings.features.threads() {
            return None;
        }
        self.article_at(path)
            .map(|(ix, article)| (ix, article.id.clone()))
    }

    /// Send to the session filed under `record`, where one is held.
    pub fn send_to_record(&mut self, record: &str, content: String, cx: &mut Context<Self>) {
        if let Some(id) = self.session_by_record(record, cx).map(|chat| chat.id) {
            self.send(id, content, cx);
        }
    }

    /// Open a session on the agent named `agent` in the project at `path`,
    /// seeded with `content`, behind whatever the window shows. Nothing
    /// happens where either is not held.
    pub fn start_in(&mut self, path: &Path, agent: &str, content: String, cx: &mut Context<Self>) {
        let entry = named(&self.settings.agents, Some(agent), agent).cloned();
        let project = self.ids.intern(path);
        if let (Some(ix), Some(entry)) = (self.project_ix(project), entry) {
            self.new_session_in(ix, entry, Some(content), false, cx);
        }
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
                if !chat.load_history() {
                    return;
                }
                chat.closed = false;
                chat.flush();
            }
        });
        self.prune_archived(cx);
    }

    pub fn rename_session(&mut self, id: u64, name: String, cx: &mut Context<Self>) {
        self.with_session(id, cx, |chat| {
            let name = name.trim();
            chat.name = (!name.is_empty()).then(|| name.to_owned());
            chat.flush();
        });
        self.prune_archived(cx);
    }

    /// An agent's rename of its own session: the user's name where the user
    /// gave one, else the agent's title.
    pub fn retitle_record(&mut self, record: &str, title: String, cx: &mut Context<Self>) {
        let Some(id) = self.session_by_record(record, cx).map(|chat| chat.id) else {
            return;
        };
        self.with_session(id, cx, |chat| {
            match chat.name {
                Some(_) => chat.name = Some(title),
                None => chat.title = title,
            }
            chat.flush();
        });
    }

    pub fn close_session(&mut self, id: u64, cx: &mut Context<Self>) {
        let Some(project) = self.project_of(id).map(|ix| &mut self.projects[ix]) else {
            return;
        };
        // Closing a session is what deletes it: leaving the file would put
        // the row back on the next launch.
        if let Some(record) = project.session(id, cx).and_then(|chat| chat.record.clone()) {
            let _ = project.store().remove_session(&record);
            let _ = session_preferences::forget_session(&project.path, &record);
        }
        project.sessions.retain(|held| held.id != id);
        if project.active == Some(id) {
            // The view chooses the next entry in sidebar order, across all
            // kinds. Storage order can put an archived session first.
            project.active = None;
        }
        cx.notify();
    }

    /// Any session, in whichever project holds it — the pump that feeds a
    /// session knows only its id, and must not care which tab it sits behind.
    pub fn session<'a>(&self, id: u64, cx: &'a App) -> Option<&'a ChatSession> {
        Some(self.session_entity(id)?.read(cx))
    }

    /// The session `id`, as the entity every view drawing it observes.
    pub fn session_entity(&self, id: u64) -> Option<&Entity<ChatSession>> {
        self.projects
            .iter()
            .find_map(|project| project.session_entity(id))
    }

    /// Every open project's sessions, as the entities they are held in.
    pub fn chat_entities(&self) -> Vec<Entity<ChatSession>> {
        self.projects
            .iter()
            .flat_map(|project| &project.sessions)
            .map(|held| held.chat.clone())
            .collect()
    }

    /// Every open project's sessions, read.
    pub fn chats<'a>(&'a self, cx: &'a App) -> impl Iterator<Item = &'a ChatSession> + 'a {
        self.projects
            .iter()
            .flat_map(move |project| project.chats(cx))
    }

    /// Read session `id`'s turns back off disk, where they were left unloaded.
    /// `false` for no such session, or turns that would not read. Loading
    /// does not notify: nothing drawn changes until the turns are read.
    pub fn load_history(&self, id: u64, cx: &mut App) -> bool {
        self.session_entity(id)
            .cloned()
            .is_some_and(|chat| chat.update(cx, |chat, _| chat.load_history()))
    }

    /// The session filed under this id, wherever it is — how a card finds the
    /// agent it was handed to after a launch that renumbered every session.
    /// [`ChatSession::id`] is minted per launch and means nothing on disk;
    /// this is the name that keeps.
    pub fn session_by_record<'a>(&'a self, record: &str, cx: &'a App) -> Option<&'a ChatSession> {
        self.chats(cx)
            .find(|chat| chat.record.as_deref() == Some(record))
    }

    /// The id a session is filed under, minted if it has none — what a
    /// card sent to it writes down.
    pub fn mint_record(&self, id: u64, cx: &mut App) -> Option<String> {
        self.update_session(id, cx, |chat| chat.mint_record().map(str::to_owned))?
    }

    /// Run `f` on the session, where it still exists, without telling the
    /// views drawing it.
    fn update_session<T>(
        &self,
        id: u64,
        cx: &mut App,
        f: impl FnOnce(&mut ChatSession) -> T,
    ) -> Option<T> {
        let chat = self.session_entity(id)?.clone();
        Some(chat.update(cx, |chat, _| f(chat)))
    }

    /// Run `f` on the session, where it still exists, and tell the views
    /// drawing it.
    pub fn with_session(&self, id: u64, cx: &mut App, f: impl FnOnce(&mut ChatSession)) {
        if let Some(chat) = self.session_entity(id).cloned() {
            chat.update(cx, |chat, cx| {
                f(chat);
                cx.notify();
            });
        }
    }

    /// Switch a session's mode — what the composer's mode picker reports.
    /// See [`ChatSession::set_mode`]. A pick in an article's session becomes
    /// the article default; one in a session an identity owns, the identity's;
    /// one in any other session, the project's default for its agent.
    pub fn set_session_mode(&mut self, id: u64, mode_id: String, cx: &mut Context<Self>) {
        let Some(chat) = self.session_entity(id).cloned() else {
            return;
        };
        let (taken, article, identity, cwd, entry) = chat.update(cx, |chat, cx| {
            let taken = chat.set_mode(&mode_id, cx);
            cx.notify();
            (
                taken,
                chat.article.is_some(),
                chat.identity.clone(),
                chat.cwd.clone(),
                chat.entry.clone(),
            )
        });
        if !taken {
            return;
        }
        if article {
            self.remember_article(&cwd, &entry, ArticleChange::Mode(mode_id), cx);
        } else if let Some(identity) = identity.filter(|id| self.identity(id).is_some()) {
            self.edit_identity(
                &identity,
                |identity| identity.choices.mode = Some(mode_id),
                cx,
            );
        } else if let Err(error) = session_preferences::remember_mode(&cwd, &entry, &mode_id) {
            chat.update(cx, |chat, cx| {
                chat.default_unsaved(error);
                cx.notify();
            });
        }
    }

    /// The same for a config option, which is where the model lives. Article
    /// defaults hold select values only, so a boolean picked in an article's
    /// session stays with that session.
    pub fn set_session_config(
        &mut self,
        id: u64,
        config_id: String,
        value: SessionConfigOptionValue,
        cx: &mut Context<Self>,
    ) {
        let Some(chat) = self.session_entity(id).cloned() else {
            return;
        };
        let (taken, article, identity, cwd, entry) = chat.update(cx, |chat, cx| {
            let taken = chat.set_config(&config_id, value.clone(), cx);
            cx.notify();
            (
                taken,
                chat.article.is_some(),
                chat.identity.clone(),
                chat.cwd.clone(),
                chat.entry.clone(),
            )
        });
        if !taken {
            return;
        }
        if !article && let Some(identity) = identity.filter(|id| self.identity(id).is_some()) {
            self.edit_identity(
                &identity,
                |identity| {
                    identity.choices.config.insert(config_id, value);
                },
                cx,
            );
        } else if article {
            if let SessionConfigOptionValue::ValueId { value } = value {
                let change = ArticleChange::Config {
                    id: config_id,
                    value: value.to_string(),
                };
                self.remember_article(&cwd, &entry, change, cx);
            }
        } else if let Err(error) =
            session_preferences::remember_config(&cwd, &entry, &config_id, value)
        {
            chat.update(cx, |chat, cx| {
                chat.default_unsaved(error);
                cx.notify();
            });
        }
    }

    /// The session the chat pane would show. Gated, and it is the gate that
    /// matters most: sessions are read back off disk when a project opens,
    /// whatever the switch says, so without this a filed transcript would put
    /// the pane on screen with no composer under it.
    pub fn active_session<'a>(&self, cx: &'a App) -> Option<&'a ChatSession> {
        self.settings
            .features
            .sessions
            .then(|| self.active_project()?.active_session(cx))
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
    pub fn reachable(&self, cx: &App) -> bool {
        self.active_session(cx)
            .is_some_and(|chat| self.reachable_session(chat))
    }

    /// [`Self::reachable`] for `chat`, in front or not.
    pub fn reachable_session(&self, chat: &ChatSession) -> bool {
        chat.live() || (chat.resumable() && self.agent_for(&chat.entry).is_some())
    }
}
