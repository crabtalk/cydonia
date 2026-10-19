//! One agent session bridged into the UI.
//!
//! The connection opens on the ACP runtime and the `Session` it yields is
//! held here, so dropping `ChatSession` tears the connection (and the agent
//! process) down. A foreground pump drains the ACP event channel in coalesced
//! batches with a 120ms frame floor while streaming — one notify per frame,
//! not per chunk.
//!
//! A session outlives its connection. [`ChatSession::flush`] writes the
//! transcript whenever a turn settles, and the agent's own session id goes
//! with it, so a relaunch reads the session back and `session/load` can pick
//! the conversation up where it stopped.

#[cfg(feature = "desktop")]
use crate::agent::acp::{self, Event, Launch, Reply, Session};
#[cfg(feature = "desktop")]
use crate::model::workspace::references;
#[cfg(feature = "desktop")]
use crate::model::{media, notify};
use crate::model::{
    session_preferences::{self, Choices, OfferCache, Offers},
    settings,
    store::{self, Store},
};
#[cfg(feature = "desktop")]
use anyhow::anyhow;
use artifact::kind::Kind;
use artifact::location::Location;
use artifact::project::Project as _;
#[cfg(feature = "desktop")]
use artifact::session::chat::ToolStatus;
use artifact::session::{
    chat::{ChatItem, PlanStatus},
    record::{ForkOrigin, Record},
};
use bezel::agent::orbs::engine::Frame;
use bezel::gpui::{App, AppContext as _, Context, Task};
#[cfg(feature = "desktop")]
use cydonia_acp::proto::{
    ContentBlock, MaybeUndefined, PlanEntryStatus, RequestPermissionRequest,
    RequestPermissionResponse, SessionUpdate, StopReason, ToolCallContent, ToolCallStatus,
};
use cydonia_acp::proto::{
    PermissionOptionKind, SessionConfigOption, SessionConfigOptionValue, SessionModeState,
};
use futures::{FutureExt as _, future::Shared};
use std::{
    cell::RefCell,
    collections::{BTreeMap, VecDeque},
    path::Path,
    rc::Rc,
    time::Duration,
};
#[cfg(feature = "desktop")]
use std::{collections::HashMap, path::PathBuf};
use web_time::{Instant, SystemTime, UNIX_EPOCH};

/// What the stand-in answers without the `desktop` feature, one per turn in
/// order and then round again.
#[cfg(not(feature = "desktop"))]
const STAND_IN: [&str; 3] = [
    "I'm a stand-in: this browser demo has no agent behind it. In the desktop app, \
     this session starts a real one — Claude Code, Codex, Gemini or any agent that \
     speaks ACP — in the project's directory, and it can read and edit the boards and \
     articles you see here.\n\nDownload Cydonia at https://cydonia.sh to try it on a \
     project of your own.",
    "I can't do that from here — nothing in this tab can reach a disk or run a \
     process. On the desktop, an agent would pick up a card like LAUNCH-5, tag it \
     busy while it works, and file what it made as an article. Open **Plan the \
     launch** in the sidebar to read one that did.",
    "Still a stand-in, I'm afraid. Everything else in this window is the real app, \
     so drag a card, edit an article, or make a board — and get the desktop app at \
     https://cydonia.sh when you want an agent that answers.",
];

#[cfg(feature = "desktop")]
const STREAM_FRAME: Duration = Duration::from_millis(120);

/// How much of an agent's stderr one block keeps. An agent that logs a line a
/// second all afternoon must not be what grows the transcript without end, and
/// the tail is the half worth keeping: what a process said last is what it
/// said about dying.
#[cfg(feature = "desktop")]
const STDERR_KEEP: usize = 16 * 1024;

/// How much of the context window the conversation has taken, as the agent
/// counts it.
///
/// Runtime only, and not written to the record: the file carries the
/// transcript, and whichever agent reads it back counts for itself.
#[derive(Clone, Copy)]
pub struct Usage {
    pub used: u64,
    pub size: u64,
}

impl Usage {
    /// How full, as a fraction. A window of no size is an agent that does not
    /// count, and there is nothing to draw for it.
    pub fn fraction(&self) -> Option<f32> {
        (self.size > 0).then(|| (self.used as f32 / self.size as f32).clamp(0., 1.))
    }
}

/// The turn in flight: when it started, and what the context had spent by then.
///
/// Runtime only, like [`Usage`], and for the same reason — the two numbers it
/// exists to count from are only read while a turn is running, and a relaunch
/// has none.
#[derive(Clone, Copy)]
struct Flight {
    at: SystemTime,
    used: u64,
}

/// One way to answer a permission request. `kind` is what decides how the
/// button paints — allow and reject must not look alike.
pub struct Choice {
    pub id: String,
    pub name: String,
    pub kind: PermissionOptionKind,
}

/// Whether the session can talk to an agent right now.
pub enum Connection {
    /// No agent process: read back from disk, archived, or given up on.
    Idle,
    Connecting,
    #[cfg(feature = "desktop")]
    Live(Box<Session>),
    /// The agent went away on its own — a crash, or a closed stdout.
    Lost,
}

/// A slash command the agent offers for this session — what to type, and what
/// it does. The description is required by the protocol, so a picker can
/// always say what a name does not.
#[derive(Clone, PartialEq, Eq)]
pub struct Command {
    pub name: String,
    pub description: String,
}

pub struct PermissionPrompt {
    pub title: String,
    pub options: Vec<Choice>,
    /// Whether the answer should stand for every call like this one, rather
    /// than for this one alone — the checkbox beside the buttons. It picks
    /// between the `*Once` and `*Always` forms of whichever button is pressed,
    /// which is what lets two buttons carry four options.
    pub always: bool,
    #[cfg(feature = "desktop")]
    reply: Reply<RequestPermissionResponse>,
}

pub struct ChatSession {
    pub number: Option<u64>,
    pub id: u64,
    pub entry: settings::Agent,
    /// The project's location, whose root is the directory the agent runs in.
    /// Kept here so a session can reconnect and file itself without asking
    /// the workspace.
    pub location: Location,
    pub connection: Connection,
    pub items: Vec<ChatItem>,
    history_unloaded: bool,
    /// [`ChatSession::preview`] as it stood when the history was unloaded.
    unloaded_preview: String,
    pub fork: Option<ForkOrigin>,
    /// See [`Record::replay`].
    pub replay: Option<usize>,
    pub draft: String,
    pub(crate) draft_save: Option<Task<()>>,
    pub sent_at: BTreeMap<usize, u64>,
    pub plan: Vec<(String, PlanStatus)>,
    pub permission: Option<PermissionPrompt>,
    pub commands: Vec<Command>,
    /// The modes the agent offers and the one it is in, as `session/new`
    /// reported them and every `CurrentModeUpdate` since.
    ///
    /// Belongs to the agent process rather than to the transcript, so it is
    /// filled when a connection goes live and left where it is when one goes
    /// away — the pane gates on [`Self::live`] rather than on this being empty.
    pub modes: Option<SessionModeState>,
    /// The same for config options, which is where the model lives — see
    /// `SessionConfigOptionCategory::Model`.
    pub config: Vec<SessionConfigOption>,
    preferences: Choices,
    /// Context spent, when the agent says.
    pub usage: Option<Usage>,
    /// What the turn in flight is counted from — see [`Self::elapsed`] and
    /// [`Self::spent`].
    flight: Option<Flight>,
    /// The agent's own name for the session, from `SessionInfoUpdate`.
    pub title: String,
    /// The name you typed, which the agent never overwrites. Two fields rather
    /// than one and a flag: whose name it is *is* the state.
    pub name: Option<String>,
    /// Last user submission, or creation time before the first message.
    pub updated: SystemTime,
    /// The agent's own id for this session — what `session/load` resumes.
    pub agent_session: Option<String>,
    /// The id this session is filed under. Minted when the session connects,
    /// before anything is written — see [`Self::filed`] for one that is on
    /// disk.
    pub record: Option<String>,
    /// The mint of [`Self::record`] while the backend has not answered it.
    filing: Option<Shared<Task<Option<String>>>>,
    /// [`Self::mint_record`] while the backend has not answered it.
    minting: Option<Shared<Task<Option<String>>>>,
    /// Whether [`Self::record`] has a file behind it.
    written: bool,
    /// Whether the user archived it. Typing into it clears this.
    pub closed: bool,
    /// The article it was opened under, by id — see [`Record::article`].
    pub article: Option<String>,
    /// Its labels, normalised — see [`artifact::label`].
    pub labels: Vec<String>,
    pub streaming: bool,
    pub(crate) last_activity: Instant,
    pub(crate) turn_started: Option<Instant>,
    pub(crate) tool_started: BTreeMap<String, Instant>,
    /// Prompts waiting for an agent to send them to: what was typed while a
    /// turn was in flight, and what a card sent to a new session opened it with.
    pub queue: VecDeque<String>,
    /// The bytes of pictures this app filed for messages, by the path the
    /// message names each at, until the message is sent. A picture named
    /// here goes to the agent from these bytes rather than from its path.
    #[cfg(feature = "desktop")]
    inline: HashMap<PathBuf, Vec<u8>>,
    /// The geometry of the sidebar's orb for this session, reused tick to
    /// tick. Its own buffer: each orb on screen is filled where it is built
    /// and read back in the paint phase.
    pub mark: Rc<RefCell<Frame>>,
    /// Moved whenever the items are replaced rather than added to, so a view
    /// holding state keyed by item index lets it go.
    pub epoch: u64,
    _pump: Task<()>,
}

impl ChatSession {
    /// The link to turns `from..=to` of this session, from 1:
    /// `cydonia://<project>#43:5-7`. Nothing for a session without a number
    /// yet.
    pub(crate) fn turn_link(&self, from: usize, to: usize) -> Option<String> {
        use artifact::reference::{self, Reference, Span, Within};
        let number = self.number?;
        let name = reference::project_name(self.cwd())?;
        let turns = Span {
            from: from.min(to) as u64,
            to: from.max(to) as u64,
        };
        Some(Reference::entry(Some(&name), number, Some(Within::Span(turns))).link())
    }

    /// Where this session is filed — its project's backend.
    fn store(&self) -> Option<Store> {
        store::open(&self.location)
    }

    /// The directory the agent runs in, on the machine the project is on.
    pub fn cwd(&self) -> &Path {
        self.location.root()
    }

    /// Open a session on `entry` in the project at `location`. `seed` is its
    /// first prompt, sent as soon as the agent is up — what a card sent to a
    /// new session rides in on.
    pub fn connect(
        id: u64,
        entry: settings::Agent,
        location: Location,
        seed: Option<String>,
        cx: &mut Context<Self>,
    ) -> Self {
        let preferences = session_preferences::load(&location, &entry, None);
        Self::open(id, entry, location, seed, preferences, cx)
    }

    /// [`Self::connect`] with the choices the launch starts from given.
    fn open(
        id: u64,
        entry: settings::Agent,
        location: Location,
        seed: Option<String>,
        preferences: Choices,
        cx: &mut Context<Self>,
    ) -> Self {
        #[cfg(feature = "desktop")]
        let (launch, connection) = (
            Launch {
                choices: preferences.clone(),
                ..Launch::new(location.clone())
            },
            Connection::Connecting,
        );
        #[cfg(not(feature = "desktop"))]
        let connection = Connection::Idle;
        let mut chat = Self {
            id,
            entry,
            location,
            connection,
            items: Vec::new(),
            history_unloaded: false,
            unloaded_preview: String::new(),
            fork: None,
            replay: None,
            draft: String::new(),
            draft_save: None,
            sent_at: BTreeMap::new(),
            plan: Vec::new(),
            permission: None,
            commands: Vec::new(),
            modes: None,
            config: Vec::new(),
            preferences,
            usage: None,
            flight: None,
            title: String::new(),
            name: None,
            updated: SystemTime::now(),
            agent_session: None,
            record: None,
            filing: None,
            minting: None,
            written: false,
            number: None,
            closed: false,
            article: None,
            labels: Vec::new(),
            streaming: false,
            last_activity: Instant::now(),
            turn_started: None,
            tool_started: BTreeMap::new(),
            queue: seed.into_iter().collect(),
            #[cfg(feature = "desktop")]
            inline: HashMap::new(),
            mark: Rc::default(),
            epoch: 0,
            _pump: Task::ready(()),
        };
        #[cfg(feature = "desktop")]
        {
            chat._pump = chat.launch(launch, cx);
        }
        #[cfg(not(feature = "desktop"))]
        drop(chat.file(cx));
        chat
    }

    /// [`Self::connect`] under the article `article` — see
    /// [`Record::article`].
    pub fn connect_article(
        id: u64,
        entry: settings::Agent,
        location: Location,
        article: String,
        choices: Choices,
        cx: &mut Context<Self>,
    ) -> Self {
        let mut chat = Self::open(id, entry, location, None, choices, cx);
        chat.article = Some(article);
        chat
    }

    /// A session read back from disk. It starts idle — a launch must not spawn
    /// an agent per session — and reconnects when it is brought to the front,
    /// or when something is sent to it.
    pub fn restore(id: u64, location: Location, entry: settings::Agent, record: Record) -> Self {
        let updated = record.at();
        let preferences = session_preferences::load(&location, &entry, Some(&record.id));
        Self {
            id,
            entry,
            location,
            connection: Connection::Idle,
            replay: record.replay,
            items: record.items,
            history_unloaded: false,
            unloaded_preview: String::new(),
            fork: record.fork,
            draft: record.draft,
            draft_save: None,
            sent_at: record.sent_at,
            plan: Vec::new(),
            permission: None,
            commands: Vec::new(),
            modes: None,
            config: Vec::new(),
            preferences,
            usage: None,
            flight: None,
            title: record.title,
            name: record.name,
            updated,
            agent_session: record.session,
            record: Some(record.id),
            filing: None,
            minting: None,
            written: true,
            number: record.number,
            closed: record.closed,
            article: record.article,
            labels: record.labels,
            streaming: false,
            last_activity: Instant::now(),
            turn_started: None,
            tool_started: BTreeMap::new(),
            queue: VecDeque::new(),
            #[cfg(feature = "desktop")]
            inline: HashMap::new(),
            mark: Rc::default(),
            epoch: 0,
            _pump: Task::ready(()),
        }
    }

    /// Whether the transcript is in memory, rather than left on disk.
    pub fn history_loaded(&self) -> bool {
        !self.history_unloaded
    }

    pub fn load_history(&mut self) -> bool {
        if !self.history_unloaded {
            return true;
        }
        let Some(record) = self
            .record
            .as_deref()
            .and_then(|id| self.store()?.session(id))
        else {
            return false;
        };
        self.replay = record.replay;
        self.items = record.items;
        self.sent_at = record.sent_at;
        self.fork = record.fork;
        self.draft = record.draft;
        self.history_unloaded = false;
        true
    }

    /// Drop only a persisted archive; failed saves retain the in-memory copy.
    pub fn unload_history(&mut self) {
        if !self.closed || self.history_unloaded || self.draft_save.is_some() {
            return;
        }
        let Some(record) = self
            .record
            .as_deref()
            .and_then(|id| self.store()?.session(id))
        else {
            return;
        };
        if serde_json::to_value(&record).ok() != serde_json::to_value(self.to_record()).ok() {
            return;
        }
        self.unloaded_preview = self.preview();
        self.items = Vec::new();
        self.sent_at = BTreeMap::new();
        self.fork = None;
        self.replay = None;
        self.draft = String::new();
        self.epoch += 1;
        self.plan = Vec::new();
        self.commands = Vec::new();
        self.config = Vec::new();
        self.modes = None;
        self.permission = None;
        self.usage = None;
        self.tool_started = BTreeMap::new();
        self.history_unloaded = true;
    }

    /// The start of the agent's last response as plain text — see
    /// [`artifact::text::preview`]. Empty before the agent has said anything.
    pub fn preview(&self) -> String {
        if self.history_unloaded {
            return self.unloaded_preview.clone();
        }
        self.items
            .iter()
            .rev()
            .find_map(|item| match item {
                ChatItem::Agent(text) if !text.trim().is_empty() => Some(text.as_str()),
                _ => None,
            })
            .map(artifact::text::preview)
            .unwrap_or_default()
    }

    /// Last user submission in milliseconds, used for sidebar ordering.
    pub fn touched(&self) -> u128 {
        self.updated
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis()
    }

    fn to_record(&self) -> Record {
        Record {
            number: self.number,
            // The file is minted before the first write, so by the time there
            // is a record to name there is a name for it.
            id: self.record.clone().unwrap_or_default(),
            agent: self.entry.name.clone(),
            agent_id: self.entry.id.clone(),
            session: self.agent_session.clone(),
            title: self.title.clone(),
            name: self.name.clone(),
            updated: self
                .updated
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
            closed: self.closed,
            labels: self.labels.clone(),
            items: self.items.clone(),
            fork: self.fork.clone(),
            article: self.article.clone(),
            replay: self.replay,
            draft: self.draft.clone(),
            sent_at: self.sent_at.clone(),
        }
    }

    pub fn fork_at(&self, id: u64, before: usize) -> Option<Self> {
        let source = if self.history_unloaded {
            self.store()?.session(self.record.as_deref()?)?
        } else {
            self.to_record()
        };
        let record = source.fork_at(before)?;
        let mut chat = Self::restore(id, self.location.clone(), self.entry.clone(), record);
        chat.record = None;
        chat.written = false;
        chat.preferences = self.preferences.clone();
        Some(chat)
    }

    /// The id this session is filed under, minted if it has none. Answers
    /// once the backend has, with `None` for a mint it refused.
    fn file(&mut self, cx: &mut Context<Self>) -> Shared<Task<Option<String>>> {
        if let Some(id) = &self.record {
            return Task::ready(Some(id.clone())).shared();
        }
        if let Some(filing) = &self.filing {
            return filing.clone();
        }
        let Some(store) = self.store() else {
            return Task::ready(None).shared();
        };
        let made = store.create_session();
        let filing = cx
            .spawn(async move |this, cx| {
                let id = made.await.ok();
                this.update(cx, |chat, _| {
                    chat.filing = None;
                    if chat.record.is_none() {
                        chat.record = id;
                    }
                    chat.record.clone()
                })
                .ok()
                .flatten()
            })
            .shared();
        self.filing = Some(filing.clone());
        filing
    }

    /// The id this session is filed under, and the number it is referred to
    /// by, each made if it has none. Answers the id once the backend has
    /// answered both.
    ///
    /// A card sent to a session writes the session down on the click that sent
    /// it, and until then a session that has said nothing has no id to be
    /// written down.
    pub fn mint_record(&mut self, cx: &mut Context<Self>) -> Shared<Task<Option<String>>> {
        if let (Some(id), Some(_)) = (&self.record, self.number) {
            return Task::ready(Some(id.clone())).shared();
        }
        if let Some(minting) = &self.minting {
            return minting.clone();
        }
        let filing = self.file(cx);
        let store = self.store();
        let minting = cx
            .spawn(async move |this, cx| {
                let id = filing.await;
                let numbered = this.read_with(cx, |chat, _| chat.number.is_some()).ok()?;
                let number = match (&id, numbered) {
                    (Some(id), false) => match &store {
                        Some(store) => store.number(Kind::Session, id).await.ok(),
                        None => None,
                    },
                    _ => None,
                };
                this.update(cx, |chat, cx| {
                    chat.minting = None;
                    if chat.number.is_none() && number.is_some() {
                        chat.number = number;
                        cx.notify();
                    }
                })
                .ok()?;
                id
            })
            .shared();
        self.minting = Some(minting.clone());
        minting
    }

    /// The id this session is filed under, once there is a file under it.
    pub fn filed(&self) -> Option<&str> {
        self.record.as_deref().filter(|_| self.written)
    }

    /// Whether nothing has been said in it yet — see [`nothing_said`].
    pub fn unsaid(&self) -> bool {
        !self.history_unloaded && nothing_said(&self.items)
    }

    /// Cut the transcript back to before the user message at `at` and return
    /// that message. The agent is restarted on a fresh session, and what is
    /// left goes to it as history with the next prompt.
    pub fn rewind(&mut self, at: usize, cx: &mut Context<Self>) -> Option<String> {
        if !self.load_history() {
            return None;
        }
        let ChatItem::User(text) = self.items.get(at)? else {
            return None;
        };
        let text = text.clone();
        self.cancel();
        self.items.truncate(at);
        self.sent_at.retain(|ix, _| *ix < at);
        self.agent_session = None;
        self.replay = Some(at);
        self.epoch += 1;
        self.streaming = false;
        self.queue.clear();
        self.plan = Vec::new();
        self.permission = None;
        self.flight = None;
        self.turn_started = None;
        self.tool_started = BTreeMap::new();
        self.connection = Connection::Idle;
        self._pump = Task::ready(());
        self.flush(cx);
        self.resume(cx);
        Some(text)
    }

    /// Give a session with panel tabs a stable identity, even before its first
    /// prompt. Answers the id it is filed under once the backend has minted
    /// it.
    pub(crate) fn retain_panel(&mut self, cx: &mut Context<Self>) -> Task<Option<String>> {
        if !self.load_history() {
            return Task::ready(None);
        }
        self.write_filed(cx)
    }

    /// Save a conversation after its first prompt or fork.
    pub fn flush(&mut self, cx: &mut Context<Self>) {
        if !self.load_history() {
            return;
        }
        if self.unsaid() && self.fork.is_none() {
            return;
        }
        self.write_filed(cx).detach();
    }

    /// Write the record, at once where it has an id and once one is minted
    /// where it has none. Answers the id.
    fn write_filed(&mut self, cx: &mut Context<Self>) -> Task<Option<String>> {
        let minted = self.mint_record(cx);
        let filed = self.record.is_some();
        if filed {
            self.write_record(cx);
        }
        cx.spawn(async move |this, cx| {
            let id = minted.await?;
            if !filed {
                this.update(cx, |chat, cx| chat.write_record(cx)).ok()?;
            }
            Some(id)
        })
    }

    fn write_record(&mut self, cx: &App) {
        if let Some(store) = self.store() {
            cx.background_spawn(store.save_session(&self.to_record()))
                .detach();
        }
        self.written = true;
        self.save_preferences();
    }

    /// Point the session at an agent again, replaying the conversation the
    /// agent still holds when it supports `session/load`.
    ///
    /// Nothing without the `desktop` feature: there is no agent to point at.
    pub fn resume(&mut self, cx: &mut Context<Self>) {
        #[cfg(not(feature = "desktop"))]
        {
            let _ = cx;
        }
        #[cfg(feature = "desktop")]
        {
            if self.closed || !self.load_history() {
                return;
            }
            let launch = Launch {
                previous: self.agent_session.clone(),
                history: (self.fork.is_some() || self.replay.is_some()).then(|| {
                    let end = self.replay.unwrap_or(usize::MAX).min(self.items.len());
                    acp::history(&self.items[..end])
                }),
                history_pending: self.replay.is_some(),
                choices: self.preferences.clone(),
                ..Launch::new(self.location.clone())
            };
            self._pump = self.launch(launch, cx);
            self.connection = Connection::Connecting;
            self.last_activity = Instant::now();
            self.closed = false;
        }
    }

    /// Start the agent on `launch`, filed under this session's id — minted
    /// first where it has none.
    #[cfg(feature = "desktop")]
    fn launch(&mut self, launch: Launch, cx: &mut Context<Self>) -> Task<()> {
        let entry = self.entry.clone();
        if let Some(record) = self.record.clone() {
            return pump(
                &entry,
                Launch {
                    record: Some(record),
                    ..launch
                },
                cx,
            );
        }
        let filing = self.file(cx);
        cx.spawn(async move |this, cx| {
            let record = filing.await;
            let Ok(pumping) =
                this.update(cx, |_, cx| pump(&entry, Launch { record, ..launch }, cx))
            else {
                return;
            };
            pumping.await;
        })
    }

    /// Close the connection and keep the transcript. Dropping the [`Session`]
    /// is what tears the agent process down — see its `Drop`.
    pub fn close(&mut self, cx: &mut Context<Self>) {
        // Before the connection goes, not after: an agent whose stdin closes
        // half way through answering has a request it can never finish, and
        // says so on the way out.
        self.cancel();
        self.connection = Connection::Idle;
        self._pump = Task::ready(());
        self.streaming = false;
        self.closed = true;
        self.queue.clear();
        self.flush(cx);
    }

    /// How long the turn in flight has been running.
    pub fn elapsed(&self) -> Option<Duration> {
        self.flight?.at.elapsed().ok()
    }

    /// What the turn in flight has spent, as the agent counts context.
    ///
    /// The difference rather than the total: `used` is the whole conversation,
    /// and what a running turn is costing is what it has added to it. `None`
    /// until an agent has counted at all — most do not until the first turn is
    /// answered, so a first turn shows its clock and nothing else.
    pub fn spent(&self) -> Option<u64> {
        Some(self.usage?.used.saturating_sub(self.flight?.used))
    }

    pub fn live(&self) -> bool {
        #[cfg(feature = "desktop")]
        return matches!(self.connection, Connection::Live(_));
        #[cfg(not(feature = "desktop"))]
        false
    }

    /// Whether sending to it would start an agent: it is not talking to one,
    /// and one is not already on the way.
    pub fn idle(&self) -> bool {
        matches!(self.connection, Connection::Idle | Connection::Lost)
    }

    /// A session whose agent is no longer in `settings.toml` reads back but
    /// cannot reconnect — there is no command left to spawn.
    pub fn resumable(&self) -> bool {
        !self.entry.command.is_empty()
    }

    /// What to call this session: your name, else the agent's, else the
    /// agent's own name.
    pub fn label(&self) -> String {
        match (&self.name, self.title.is_empty()) {
            (Some(name), _) => name.clone(),
            (None, false) => self.title.clone(),
            (None, true) => self.entry.name.clone(),
        }
    }

    /// Send the picture a message names at `path` from `bytes`, the bytes this
    /// app filed there.
    #[cfg(feature = "desktop")]
    pub fn hold_picture(&mut self, path: PathBuf, bytes: Vec<u8>) {
        self.inline.insert(path, bytes);
    }

    /// Send now, or queue it for whenever there is an agent to send it to —
    /// a turn in flight, or a connection still being made.
    pub fn send(&mut self, content: String, cx: &mut Context<Self>) {
        if self.closed {
            return;
        }
        self.updated = SystemTime::now();
        // No agent to wait for: the stand-in answers at once.
        #[cfg(not(feature = "desktop"))]
        self.prompt(content, cx);
        #[cfg(feature = "desktop")]
        if self.streaming || !self.live() {
            self.queue.push_back(content);
            self.flush(cx);
        } else {
            self.prompt(content, cx);
        }
    }

    /// Send the next queued prompt, if there is one and nothing is in flight.
    pub fn drain(&mut self, cx: &mut Context<Self>) {
        if self.closed || self.streaming {
            return;
        }
        if let Some(next) = self.queue.pop_front() {
            self.prompt(next, cx);
        }
    }

    /// Send `content` once this session has the id and the number the agent
    /// is told it by.
    fn prompt(&mut self, content: String, cx: &mut Context<Self>) {
        #[cfg(feature = "desktop")]
        if self.record.is_none() || self.number.is_none() {
            let minted = self.mint_record(cx);
            cx.spawn(async move |this, cx| {
                minted.await;
                let _ = this.update(cx, |chat, cx| chat.deliver(content, cx));
            })
            .detach();
            return;
        }
        self.deliver(content, cx);
    }

    fn deliver(&mut self, content: String, cx: &mut Context<Self>) {
        #[cfg(not(feature = "desktop"))]
        {
            let turn = self
                .items
                .iter()
                .filter(|item| matches!(item, ChatItem::User(_)))
                .count();
            self.sent_at
                .insert(self.items.len(), artifact::stamp::seconds());
            self.items.push(ChatItem::User(content));
            self.items
                .push(ChatItem::Agent(STAND_IN[turn % STAND_IN.len()].to_owned()));
            self.flush(cx);
        }
        #[cfg(feature = "desktop")]
        {
            let Connection::Live(session) = &self.connection else {
                self.queue.push_front(content);
                return;
            };
            if self.streaming {
                self.queue.push_front(content);
                return;
            }
            let article = self
                .article
                .as_deref()
                .and_then(|id| self.store()?.article(id))
                .and_then(|article| article.number);
            let inline = media::attached(&content)
                .into_iter()
                .filter_map(|path| {
                    let bytes = self.inline.remove(&path)?;
                    Some((path, bytes))
                })
                .collect();
            session.prompt(&content, self.number, article, inline);
            self.last_activity = Instant::now();
            self.turn_started = Some(self.last_activity);
            self.tool_started.clear();
            self.flight = Some(Flight {
                at: SystemTime::now(),
                used: self.usage.map_or(0, |usage| usage.used),
            });
            self.sent_at
                .insert(self.items.len(), artifact::stamp::seconds());
            self.items.push(ChatItem::User(content));
            self.streaming = true;
            // Before the answer, not just after it: what you said is not the
            // agent's to lose if the turn never finishes.
            self.flush(cx);
        }
    }

    /// Cancel the in-flight turn. A pending permission request MUST be
    /// answered `Cancelled` per spec before `session/cancel` goes out.
    pub fn cancel(&mut self) {
        #[cfg(feature = "desktop")]
        if let Some(prompt) = self.permission.take() {
            prompt.reply.send(RequestPermissionResponse::cancelled());
        }
        #[cfg(feature = "desktop")]
        if let Connection::Live(session) = &self.connection
            && let Err(e) = session.cancel()
        {
            self.notice(true, &format!("cancel failed: {}", acp::error_text(&e)));
        }
    }

    /// Switch the agent's mode (`session/set_mode`).
    ///
    /// Applied here as well as sent. The request is fire-and-forget and the
    /// agent's `CurrentModeUpdate` is the confirmation, so a picker that waited
    /// for it would read as a press that did nothing — and the update, when it
    /// lands, is what settles any disagreement.
    ///
    /// Answers whether the agent offers `mode_id`. The pick is kept as this
    /// session's choice only; which default it also becomes is the caller's.
    pub fn set_mode(&mut self, mode_id: &str, cx: &App) -> bool {
        let offered = self.offered(cx).modes.is_some_and(|modes| {
            modes
                .available_modes
                .iter()
                .any(|mode| &*mode.id == mode_id)
        });
        if !offered {
            return false;
        }
        #[cfg(feature = "desktop")]
        if let Connection::Live(session) = &self.connection {
            session.set_mode(mode_id);
        }
        if self.live()
            && let Some(modes) = self.modes.as_mut()
        {
            modes.current_mode_id = mode_id.into();
        }
        self.preferences.mode = Some(mode_id.into());
        self.save_preferences();
        true
    }

    /// Say that a pick taken here could not be kept as a default.
    pub fn default_unsaved(&mut self, error: impl std::fmt::Display) {
        self.notice(true, &format!("Could not save session default: {error}"));
    }

    /// The same for a config option, which is where the model lives
    /// (`session/set_config_option`).
    pub fn set_config(
        &mut self,
        config_id: &str,
        value: SessionConfigOptionValue,
        cx: &App,
    ) -> bool {
        let offered = self.offered(cx).config.iter().any(|option| {
            &*option.id == config_id && session_preferences::supports(option, &value)
        });
        if !offered {
            return false;
        }
        #[cfg(feature = "desktop")]
        if let Connection::Live(session) = &self.connection {
            session.set_config_option(config_id, value.clone());
        }
        if self.live()
            && let Some(option) = self
                .config
                .iter_mut()
                .find(|option| &*option.id == config_id)
        {
            session_preferences::adopt(option, value.clone());
        }
        self.preferences.config.insert(config_id.into(), value);
        self.save_preferences();
        true
    }

    /// What the agent offers this session, and what it is on: the live
    /// connection's while there is one, otherwise the [`OfferCache`] with this
    /// session's choices on.
    pub fn offered(&self, cx: &App) -> Offers {
        if self.live() {
            Offers {
                modes: self.modes.clone(),
                config: self.config.clone(),
            }
        } else {
            OfferCache::get(cx, &self.location, &self.entry).with(&self.preferences)
        }
    }

    fn save_preferences(&mut self) {
        if let Some(record) = &self.record
            && let Err(error) = session_preferences::remember_session(
                &self.location,
                &self.entry,
                record,
                &self.preferences,
            )
        {
            self.notice(true, &format!("Could not save session choices: {error}"));
        }
    }

    /// Answer the pending permission prompt with the chosen option id.
    pub fn respond_permission(&mut self, option_id: String) {
        #[cfg(not(feature = "desktop"))]
        let _ = option_id;
        #[cfg(feature = "desktop")]
        if let Some(prompt) = self.permission.take() {
            prompt
                .reply
                .send(RequestPermissionResponse::selected(option_id));
        }
    }

    /// Flip whether the pending prompt is answered once or for good.
    pub fn toggle_permission_always(&mut self) {
        if let Some(prompt) = &mut self.permission {
            prompt.always = !prompt.always;
        }
    }

    /// Drop the `ix`th waiting prompt — a steer taken back before the turn in
    /// flight got to it.
    pub fn unqueue(&mut self, ix: usize) {
        self.queue.remove(ix);
    }

    #[cfg(feature = "desktop")]
    fn apply(&mut self, event: Event, cx: &mut Context<Self>) {
        if self.closed {
            return;
        }
        self.last_activity = Instant::now();
        match event {
            Event::Update(update) => self.apply_update(update, cx),
            Event::Permission(request, reply) => self.open_permission(request, reply),
            Event::Stderr(line) => self.stderr(line),
            Event::TurnDone(result) => {
                if result.is_ok() {
                    self.replay = None;
                }
                self.finish_thinking();
                self.streaming = false;
                match result {
                    Ok(StopReason::EndTurn) => {}
                    Ok(StopReason::Cancelled) => {
                        self.fail_running_tools();
                        self.notice(false, "cancelled");
                    }
                    Ok(StopReason::Refusal) => self.notice(false, "the agent refused to continue"),
                    Ok(StopReason::MaxTokens) => self.notice(false, "stopped: max tokens"),
                    Ok(StopReason::MaxTurnRequests) => {
                        self.notice(false, "stopped: max turn requests")
                    }
                    Ok(other) => self.notice(false, &format!("stopped: {other:?}")),
                    Err(e) => {
                        self.fail_running_tools();
                        self.notice(true, &format!("turn failed: {}", acp::error_text(&e)));
                    }
                }
                self.flush(cx);
                self.drain(cx);
            }
            Event::Closed => {
                self.connection = Connection::Lost;
                self.streaming = false;
                self.fail_running_tools();
                self.notice(true, "agent connection lost");
                self.flush(cx);
            }
        }
    }

    #[cfg(feature = "desktop")]
    fn apply_update(&mut self, update: SessionUpdate, cx: &mut Context<Self>) {
        match update {
            SessionUpdate::AgentMessageChunk(chunk) => {
                self.finish_thinking();
                let text = content_text(&chunk.content);
                if let Some(ChatItem::Agent(body)) = self.items.last_mut() {
                    body.push_str(&text);
                } else {
                    self.items.push(ChatItem::Agent(text));
                }
            }
            SessionUpdate::AgentThoughtChunk(chunk) => {
                let text = content_text(&chunk.content);
                if let Some(ChatItem::Thinking {
                    text: body,
                    done: false,
                }) = self.items.last_mut()
                {
                    body.push_str(&text);
                } else {
                    self.items.push(ChatItem::Thinking { text, done: false });
                }
            }
            SessionUpdate::ToolCall(call) => {
                self.tool_started
                    .entry(call.tool_call_id.to_string())
                    .or_insert_with(Instant::now);
                self.finish_thinking();
                self.items.push(ChatItem::Tool {
                    id: call.tool_call_id.to_string(),
                    kind: call.kind,
                    label: call.title,
                    status: tool_status(call.status),
                    output: tool_content_text(&call.content),
                });
            }
            SessionUpdate::ToolCallUpdate(update) => {
                let id = update.tool_call_id.to_string();
                let Some(ix) = self.items.iter().rposition(
                    |item| matches!(item, ChatItem::Tool { id: tool, .. } if *tool == id),
                ) else {
                    return;
                };
                let ChatItem::Tool {
                    kind,
                    label,
                    status,
                    output,
                    ..
                } = &mut self.items[ix]
                else {
                    unreachable!("rposition matched a Tool item");
                };
                if let Some(title) = update.fields.title {
                    *label = title;
                }
                if let Some(new_kind) = update.fields.kind {
                    *kind = new_kind;
                }
                if let Some(content) = update.fields.content {
                    let text = tool_content_text(&content);
                    if !text.is_empty() {
                        if !output.is_empty() {
                            output.push('\n');
                        }
                        output.push_str(&text);
                    }
                }
                if let Some(new_status) = update.fields.status {
                    *status = tool_status(new_status);
                }
            }
            SessionUpdate::SessionInfoUpdate(info) => {
                let title = match info.title {
                    MaybeUndefined::Value(title) => title,
                    MaybeUndefined::Null => String::new(),
                    MaybeUndefined::Undefined => return,
                };
                // Written now: agents name a session after the turn that
                // settled it, so no later flush is sure to carry it.
                if title != self.title {
                    self.title = title;
                    self.flush(cx);
                }
            }
            SessionUpdate::AvailableCommandsUpdate(cmds) => {
                self.commands = cmds
                    .available_commands
                    .into_iter()
                    .map(|c| Command {
                        name: c.name,
                        description: c.description,
                    })
                    .collect();
            }
            SessionUpdate::Plan(plan) => {
                // Plans arrive as full snapshots: replace, don't append.
                self.plan = plan
                    .entries
                    .into_iter()
                    .map(|entry| {
                        let status = match entry.status {
                            PlanEntryStatus::Completed => PlanStatus::Done,
                            PlanEntryStatus::InProgress => PlanStatus::Active,
                            _ => PlanStatus::Pending,
                        };
                        (entry.content, status)
                    })
                    .collect();
            }
            SessionUpdate::CurrentModeUpdate(update) => {
                if let Some(modes) = &mut self.modes {
                    modes.current_mode_id = update.current_mode_id;
                }
                self.preferences = Choices::capture(self.modes.as_ref(), &self.config);
                self.save_preferences();
            }
            // Reported whole, like the plan: replace, don't merge.
            SessionUpdate::ConfigOptionUpdate(update) => {
                self.config = update.config_options;
                self.preferences = Choices::capture(self.modes.as_ref(), &self.config);
                self.save_preferences();
            }
            SessionUpdate::UsageUpdate(update) => {
                self.usage = Some(Usage {
                    used: update.used,
                    size: update.size,
                });
            }
            // We echo the user's message locally.
            SessionUpdate::UserMessageChunk(_) => {}
            _ => {}
        }
    }

    #[cfg(feature = "desktop")]
    fn open_permission(
        &mut self,
        request: RequestPermissionRequest,
        reply: Reply<RequestPermissionResponse>,
    ) {
        let options: Vec<Choice> = request
            .options
            .into_iter()
            .map(|opt| Choice {
                id: opt.option_id.to_string(),
                name: opt.name,
                kind: opt.kind,
            })
            .collect();
        if options.is_empty() {
            reply.send(RequestPermissionResponse::cancelled());
            return;
        }
        // A replaced prompt must still be answered — an unanswered
        // reply hangs the agent.
        if let Some(previous) = self.permission.take() {
            previous.reply.send(RequestPermissionResponse::cancelled());
        }
        let title = request
            .tool_call
            .fields
            .title
            .clone()
            .unwrap_or_else(|| "Permission required".to_owned());
        self.permission = Some(PermissionPrompt {
            title,
            options,
            always: false,
            reply,
        });
    }

    /// Add one line of the agent's own stderr to the block that carries it.
    ///
    /// One block per run of lines, coalesced the way a message chunk is: the
    /// process talks in lines but it is one execution, and a block per line
    /// would be a transcript of nothing else. A run that something else
    /// interrupts starts a new one, which is what keeps the output next to
    /// whatever it was complaining about.
    #[cfg(feature = "desktop")]
    pub(crate) fn stderr(&mut self, line: String) {
        if let Some(ChatItem::Process { output, .. }) = self.items.last_mut() {
            output.push('\n');
            output.push_str(&line);
            trim_front(output, STDERR_KEEP);
            return;
        }
        self.items.push(ChatItem::Process {
            command: command_line(&self.entry),
            output: line,
        });
    }

    pub(crate) fn notice(&mut self, failed: bool, text: &str) {
        self.items.push(ChatItem::Notice {
            text: text.to_owned(),
            failed,
        });
    }

    #[cfg(feature = "desktop")]
    fn finish_thinking(&mut self) {
        if let Some(ChatItem::Thinking { done, .. }) = self.items.last_mut() {
            *done = true;
        }
    }

    #[cfg(feature = "desktop")]
    fn fail_running_tools(&mut self) {
        for item in &mut self.items {
            if let ChatItem::Tool { status, .. } = item
                && *status == ToolStatus::Running
            {
                *status = ToolStatus::Failure;
            }
        }
    }
}

pub use artifact::session::chat::nothing_said;

/// What was run, as a command line — the head of the block its output fills.
pub fn command_line(entry: &settings::Agent) -> String {
    std::iter::once(entry.command.as_str())
        .chain(entry.args.iter().map(String::as_str))
        .collect::<Vec<_>>()
        .join(" ")
}

/// Hold `text` to `keep` bytes by dropping whole lines off the front.
///
/// Whole lines, and on a char boundary either way: half a line of output reads
/// as a different error than the one that was printed.
pub fn trim_front(text: &mut String, keep: usize) {
    if text.len() <= keep {
        return;
    }
    // Where the overflow ends — walked forward to a boundary before anything
    // is sliced at it, because the byte `keep` lands on can be the middle of a
    // character and slicing there is a panic, not a short read. The end of the
    // string is always a boundary, so this terminates.
    let mut from = text.len() - keep;
    while !text.is_char_boundary(from) {
        from += 1;
    }
    let cut = match text[from..].find('\n') {
        Some(at) => from + at + 1,
        // One line longer than the whole budget, so there is no line break to
        // cut at. Its tail is still the part that matters, and `from` is a
        // boundary already.
        None => from,
    };
    text.drain(..cut);
}

#[cfg(feature = "desktop")]
fn tool_status(status: ToolCallStatus) -> ToolStatus {
    match status {
        ToolCallStatus::Completed => ToolStatus::Success,
        ToolCallStatus::Failed => ToolStatus::Failure,
        _ => ToolStatus::Running,
    }
}

#[cfg(feature = "desktop")]
fn tool_content_text(content: &[ToolCallContent]) -> String {
    content
        .iter()
        .map(|c| match c {
            ToolCallContent::Content { content } => content_text(content),
            ToolCallContent::Diff(diff) => format!("edited {}", diff.path.display()),
            ToolCallContent::Terminal { .. } => "[terminal]".to_owned(),
            _ => "[content]".to_owned(),
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(feature = "desktop")]
fn content_text(block: &ContentBlock) -> String {
    match block {
        ContentBlock::Text(text) => text.text.clone(),
        _ => "[non-text content]".to_owned(),
    }
}

#[cfg(feature = "desktop")]
/// Drain the ACP event channel into the session, for as long as there is one.
///
/// The launch carries the previous session and its saved choices.
fn pump(entry: &settings::Agent, launch: Launch, cx: &mut Context<ChatSession>) -> Task<()> {
    let entry = entry.clone();
    // Ours, not the launch's — a launch that fails still wrote to its stderr,
    // and a receiver the launch owned would be dropped with the error.
    let (tx, mut events) = acp::channel();
    let echo = tx.clone();
    let mut conn = PendingConnection(
        acp::runtime().spawn(async move { Session::spawn(&entry, launch, tx).await }),
    );

    cx.spawn(async move |this, cx| {
        let opened = (&mut conn.0)
            .await
            .unwrap_or_else(|e| Err(anyhow!("the connection task panicked: {e}")));
        let session = match opened {
            Ok(session) => session,
            Err(e) => {
                // The process is already dead — its `Child` is killed on the
                // way out of `spawn` — but the task reading its stderr is on
                // the other runtime and its last line may still be in flight.
                // One frame is what that costs, on a path that has nothing
                // left to be quick for.
                cx.background_executor().timer(STREAM_FRAME).await;
                let mut said = Vec::new();
                while let Ok(event) = events.try_recv() {
                    if let Event::Stderr(line) = event {
                        said.push(line);
                    }
                }
                let _ = this.update(cx, |chat, cx| {
                    chat.connection = Connection::Lost;
                    // What it said before the verdict, then the verdict.
                    for line in said {
                        chat.stderr(line);
                    }
                    chat.notice(true, &format!("connection failed: {e:#}"));
                    cx.notify();
                });
                return;
            }
        };
        if session.loaded {
            acp::spend_replay(&mut events, &echo);
        }
        drop(echo);

        if this
            .update(cx, |chat, cx| {
                if !chat.closed {
                    chat.agent_session = Some(session.session_id.to_string());
                    // What this agent lets you switch, read off `session/new`
                    // before the session is boxed away. Every change after
                    // this arrives as an update.
                    chat.modes = session.response.modes.clone();
                    chat.config = session.response.config_options.clone().unwrap_or_default();
                    // Picks made while it was starting were not in its launch.
                    // Put them on now, where the agent offers them.
                    let pending = chat.preferences.clone();
                    if let Some(mode) = pending.mode.as_deref()
                        && let Some(modes) = chat.modes.as_mut()
                        && modes.available_modes.iter().any(|m| &*m.id == mode)
                        && &*modes.current_mode_id != mode
                    {
                        session.set_mode(mode);
                        modes.current_mode_id = mode.into();
                    }
                    for option in &mut chat.config {
                        let Some(value) = pending.config.get(&*option.id) else {
                            continue;
                        };
                        if session_preferences::supports(option, value)
                            && session_preferences::current(option) != *value
                        {
                            session.set_config_option(&option.id, value.clone());
                            session_preferences::adopt(option, value.clone());
                        }
                    }
                    chat.preferences = Choices::capture(chat.modes.as_ref(), &chat.config);
                    let offers = Offers {
                        modes: chat.modes.clone(),
                        config: chat.config.clone(),
                    };
                    if let Err(error) =
                        OfferCache::remember(cx, &chat.location, &chat.entry, offers)
                    {
                        chat.notice(true, &format!("Could not save agent options: {error}"));
                    }
                    chat.connection = Connection::Live(Box::new(session));
                }
                // Whatever was typed while it had no agent goes now, and the
                // agent's own id is written down for a later launch to load.
                chat.drain(cx);
                chat.flush(cx);
                cx.notify();
            })
            .is_err()
        {
            return;
        }

        while let Some(event) = events.recv().await {
            let mut batch = vec![event];
            while let Ok(event) = events.try_recv() {
                batch.push(event);
            }
            let streaming = this.update(cx, |chat, cx| {
                let was_streaming = chat.streaming;
                for event in batch {
                    chat.apply(event, cx);
                }
                cx.notify();
                // The turn's end, as the only place that sees the whole batch:
                // a session read from outside cannot tell a turn that has just
                // stopped from one that was never running.
                if was_streaming && !chat.streaming {
                    let notify = references::current(cx)
                        .is_some_and(|workspace| workspace.read(cx).settings.notify_turns);
                    notify::turn_finished(chat, notify, cx);
                }
                chat.streaming
            });
            match streaming {
                Ok(true) => cx.background_executor().timer(STREAM_FRAME).await,
                Ok(false) => {}
                Err(_) => return,
            }
        }
    })
}

#[cfg(feature = "desktop")]
/// Dropping a Tokio join handle alone detaches the startup task.
struct PendingConnection(tokio::task::JoinHandle<anyhow::Result<Session>>);

#[cfg(feature = "desktop")]
impl Drop for PendingConnection {
    fn drop(&mut self) {
        self.0.abort();
    }
}

#[cfg(test)]
#[path = "../../tests/unit/session.rs"]
mod tests;
