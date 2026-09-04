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

use crate::{
    agent::acp::{self, Event, Launch, Reply, Session},
    model::{
        record::{self, Record},
        settings,
        workspace::Workspace,
    },
    view::component::transcript,
};
use anyhow::anyhow;
use bezel::gpui::{Context, Task};
use cacp::schema::{
    ContentBlock, MaybeUndefined, PermissionOptionKind, PlanEntryStatus, RequestPermissionRequest,
    RequestPermissionResponse, SessionUpdate, StopReason, ToolCallContent, ToolCallStatus,
    ToolKind,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::VecDeque,
    path::PathBuf,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

const STREAM_FRAME: Duration = Duration::from_millis(120);

#[derive(Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum ToolStatus {
    Running,
    Success,
    Failure,
}

#[derive(Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum PlanStatus {
    Pending,
    Active,
    Done,
}

#[derive(Clone, Serialize, Deserialize)]
pub enum ChatItem {
    User(String),
    Agent(String),
    Thinking {
        text: String,
        done: bool,
    },
    Tool {
        id: String,
        kind: ToolKind,
        label: String,
        status: ToolStatus,
        output: String,
    },
    /// Something the session has to say for itself: a stop reason, or a
    /// failure. `failed` picks which strip it paints as.
    Notice {
        text: String,
        failed: bool,
    },
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
    Live(Box<Session>),
    /// The agent went away on its own — a crash, or a closed stdout.
    Lost,
}

pub struct PermissionPrompt {
    pub title: String,
    pub options: Vec<Choice>,
    reply: Reply<RequestPermissionResponse>,
}

pub struct ChatSession {
    pub id: u64,
    pub entry: settings::Agent,
    /// The directory the agent runs in, which is the project's. Kept here so
    /// a session can reconnect and file itself without asking the workspace.
    pub cwd: PathBuf,
    pub connection: Connection,
    pub items: Vec<ChatItem>,
    pub plan: Vec<(String, PlanStatus)>,
    pub permission: Option<PermissionPrompt>,
    pub commands: Vec<String>,
    /// The agent's own name for the session, from `SessionInfoUpdate`.
    pub title: String,
    /// The name you typed, which the agent never overwrites. Two fields rather
    /// than one and a flag: whose name it is *is* the state.
    pub name: Option<String>,
    /// When the session last had something to say. Wall clock, not `Instant`,
    /// because the file has to carry it across a launch.
    pub updated: SystemTime,
    /// The agent's own id for this session — what `session/load` resumes.
    pub agent_session: Option<String>,
    /// Where the session is written, once it has anything to write.
    pub file: Option<PathBuf>,
    /// Whether the user archived it. Typing into it clears this.
    pub closed: bool,
    pub streaming: bool,
    /// Prompts waiting for an agent to send them to: what was typed while a
    /// turn was in flight, and what a dispatched card opened the session with.
    pub queue: VecDeque<String>,
    pub transcript: transcript::State,
    _pump: Task<()>,
}

impl ChatSession {
    /// Open a session on `entry`. `seed` is its first prompt, sent as soon as
    /// the agent is up — what a dispatched card rides in on.
    pub fn connect(
        id: u64,
        entry: settings::Agent,
        cwd: PathBuf,
        seed: Option<String>,
        cx: &mut Context<Workspace>,
    ) -> Self {
        let pump = pump(id, &entry, cwd.clone(), None, cx);
        Self {
            id,
            entry,
            cwd,
            connection: Connection::Connecting,
            items: Vec::new(),
            plan: Vec::new(),
            permission: None,
            commands: Vec::new(),
            title: String::new(),
            name: None,
            updated: SystemTime::now(),
            agent_session: None,
            file: None,
            closed: false,
            streaming: false,
            queue: seed.into_iter().collect(),
            transcript: transcript::State::default(),
            _pump: pump,
        }
    }

    /// A session read back from disk. It starts idle — a launch must not spawn
    /// an agent per session — and reconnects when something is sent to it.
    pub fn restore(
        id: u64,
        file: PathBuf,
        cwd: PathBuf,
        entry: settings::Agent,
        record: Record,
    ) -> Self {
        let updated = record.at();
        Self {
            id,
            entry,
            cwd,
            connection: Connection::Idle,
            items: record.items,
            plan: Vec::new(),
            permission: None,
            commands: Vec::new(),
            title: record.title,
            name: record.name,
            updated,
            agent_session: record.session,
            file: Some(file),
            closed: record.closed,
            streaming: false,
            queue: VecDeque::new(),
            transcript: transcript::State::default(),
            _pump: Task::ready(()),
        }
    }

    fn to_record(&self) -> Record {
        Record {
            agent: self.entry.name.clone(),
            session: self.agent_session.clone(),
            title: self.title.clone(),
            name: self.name.clone(),
            updated: self
                .updated
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
            closed: self.closed,
            items: self.items.clone(),
        }
    }

    /// Write the session out. The file is minted on the first write and not
    /// before — opening a project must not put a `.cydonia/` in it.
    pub fn flush(&mut self) {
        if self.items.is_empty() {
            return;
        }
        if self.file.is_none() {
            self.file = record::create(&self.cwd);
        }
        if let Some(file) = &self.file {
            record::write(file, &self.to_record());
        }
    }

    /// Point the session at an agent again, replaying the conversation the
    /// agent still holds when it supports `session/load`.
    pub fn resume(&mut self, cx: &mut Context<Workspace>) {
        self._pump = pump(
            self.id,
            &self.entry,
            self.cwd.clone(),
            self.agent_session.clone(),
            cx,
        );
        self.connection = Connection::Connecting;
        self.closed = false;
    }

    /// Close the connection and keep the transcript. Dropping the [`Session`]
    /// is what tears the agent process down.
    pub fn close(&mut self) {
        self.connection = Connection::Idle;
        self._pump = Task::ready(());
        self.streaming = false;
        self.closed = true;
        self.queue.clear();
        self.flush();
    }

    pub fn live(&self) -> bool {
        matches!(self.connection, Connection::Live(_))
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

    /// Send now, or queue it for whenever there is an agent to send it to —
    /// a turn in flight, or a connection still being made.
    pub fn send(&mut self, content: String) {
        if self.streaming || !self.live() {
            self.queue.push_back(content);
        } else {
            self.prompt(content);
        }
    }

    /// Send the next queued prompt, if there is one and nothing is in flight.
    pub fn drain(&mut self) {
        if self.streaming {
            return;
        }
        if let Some(next) = self.queue.pop_front() {
            self.prompt(next);
        }
    }

    fn prompt(&mut self, content: String) {
        let Connection::Live(session) = &self.connection else {
            self.queue.push_front(content);
            return;
        };
        session.prompt(&content);
        self.items.push(ChatItem::User(content));
        self.updated = SystemTime::now();
        self.streaming = true;
        // Before the answer, not just after it: what you said is not the
        // agent's to lose if the turn never finishes.
        self.flush();
    }

    /// Cancel the in-flight turn. A pending permission request MUST be
    /// answered `Cancelled` per spec before `session/cancel` goes out.
    pub fn cancel(&mut self) {
        if let Some(prompt) = self.permission.take() {
            prompt.reply.send(RequestPermissionResponse::cancelled());
        }
        if let Connection::Live(session) = &self.connection
            && let Err(e) = session.cancel()
        {
            self.notice(true, &format!("cancel failed: {}", acp::error_text(&e)));
        }
    }

    /// Answer the pending permission prompt with the chosen option id.
    pub fn respond_permission(&mut self, option_id: String) {
        if let Some(prompt) = self.permission.take() {
            prompt
                .reply
                .send(RequestPermissionResponse::selected(option_id));
        }
    }

    fn apply(&mut self, event: Event) {
        self.updated = SystemTime::now();
        match event {
            Event::Update(update) => self.apply_update(update),
            Event::Permission(request, reply) => self.open_permission(request, reply),
            Event::TurnDone(result) => {
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
                self.flush();
                self.drain();
            }
            Event::Closed => {
                self.connection = Connection::Lost;
                self.streaming = false;
                self.fail_running_tools();
                self.notice(true, "agent connection lost");
                self.flush();
            }
        }
    }

    fn apply_update(&mut self, update: SessionUpdate) {
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
            SessionUpdate::SessionInfoUpdate(info) => match info.title {
                MaybeUndefined::Value(title) => self.title = title,
                MaybeUndefined::Null => self.title.clear(),
                MaybeUndefined::Undefined => {}
            },
            SessionUpdate::AvailableCommandsUpdate(cmds) => {
                self.commands = cmds
                    .available_commands
                    .into_iter()
                    .map(|c| c.name)
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
            // We echo the user's message locally.
            SessionUpdate::UserMessageChunk(_) => {}
            _ => {}
        }
    }

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
            reply,
        });
    }

    pub(crate) fn notice(&mut self, failed: bool, text: &str) {
        self.items.push(ChatItem::Notice {
            text: text.to_owned(),
            failed,
        });
    }

    fn finish_thinking(&mut self) {
        if let Some(ChatItem::Thinking { done, .. }) = self.items.last_mut() {
            *done = true;
        }
    }

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

fn tool_status(status: ToolCallStatus) -> ToolStatus {
    match status {
        ToolCallStatus::Completed => ToolStatus::Success,
        ToolCallStatus::Failed => ToolStatus::Failure,
        _ => ToolStatus::Running,
    }
}

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

fn content_text(block: &ContentBlock) -> String {
    match block {
        ContentBlock::Text(text) => text.text.clone(),
        _ => "[non-text content]".to_owned(),
    }
}

/// Drain the ACP event channel into the session, for as long as there is one.
///
/// `previous` is the agent's own session id: with it set and the agent
/// capable, the conversation is loaded rather than started over, so the turn
/// that follows carries everything said before it.
fn pump(
    id: u64,
    entry: &settings::Agent,
    cwd: PathBuf,
    previous: Option<String>,
    cx: &mut Context<Workspace>,
) -> Task<()> {
    let entry = entry.clone();
    let conn = acp::runtime().spawn(async move {
        Session::spawn(
            &entry,
            Launch {
                previous,
                ..Launch::new(cwd)
            },
        )
        .await
    });

    cx.spawn(async move |this, cx| {
        let opened = conn
            .await
            .unwrap_or_else(|e| Err(anyhow!("the connection task panicked: {e}")));
        let (session, mut events) = match opened {
            Ok(pair) => pair,
            Err(e) => {
                let _ = this.update(cx, |workspace, cx| {
                    workspace.with_session(id, cx, |chat| {
                        chat.connection = Connection::Lost;
                        chat.notice(true, &format!("connection failed: {e:#}"));
                    });
                });
                return;
            }
        };

        if this
            .update(cx, |workspace, cx| {
                workspace.with_session(id, cx, |chat| {
                    chat.agent_session = Some(session.session_id.to_string());
                    chat.connection = Connection::Live(Box::new(session));
                });
                workspace.session_connected(id, cx);
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
            let streaming = this.update(cx, |workspace, cx| {
                workspace.with_session(id, cx, |chat| {
                    for event in batch {
                        chat.apply(event);
                    }
                });
                workspace.session(id).is_some_and(|chat| chat.streaming)
            });
            match streaming {
                Ok(true) => cx.background_executor().timer(STREAM_FRAME).await,
                Ok(false) => {}
                Err(_) => return,
            }
        }
    })
}
