//! One live agent session bridged into the UI.
//!
//! The connection opens on the ACP runtime and the `Session` it yields is
//! held here, so dropping `ChatSession` tears the connection (and the agent
//! process) down. A foreground pump drains the ACP event channel in coalesced
//! batches with a 120ms frame floor while streaming — one notify per frame,
//! not per chunk.

use crate::{
    acp::{self, Event, Reply, Session},
    app::Cydonia,
    settings, transcript,
};
use anyhow::anyhow;
use bezel::motion::Painter;
use cacp::schema::{
    ContentBlock, MaybeUndefined, PermissionOptionKind, PlanEntryStatus, RequestPermissionRequest,
    RequestPermissionResponse, SessionUpdate, StopReason, ToolCallContent, ToolCallStatus,
    ToolKind,
};
use gpui::{Context, Task};
use std::{collections::VecDeque, path::PathBuf, time::Duration};

const STREAM_FRAME: Duration = Duration::from_millis(120);

#[derive(Clone, Copy, PartialEq)]
pub enum ToolStatus {
    Running,
    Success,
    Failure,
}

#[derive(Clone, Copy, PartialEq)]
pub enum PlanStatus {
    Pending,
    Active,
    Done,
}

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

pub struct PermissionPrompt {
    pub title: String,
    pub options: Vec<Choice>,
    reply: Reply<RequestPermissionResponse>,
}

pub struct ChatSession {
    pub id: u64,
    pub entry: settings::Agent,
    pub session: Option<Session>,
    pub items: Vec<ChatItem>,
    pub plan: Vec<(String, PlanStatus)>,
    pub permission: Option<PermissionPrompt>,
    pub commands: Vec<String>,
    pub title: String,
    pub streaming: bool,
    pub lost: bool,
    pub queue: VecDeque<String>,
    /// A prompt to send the moment the session is up — the card that opened
    /// it. Taken by [`crate::app::Cydonia::session_connected`], never resent.
    pub seed: Option<String>,
    pub transcript: transcript::State,
    _pump: Task<()>,
}

impl ChatSession {
    pub fn connect(
        id: u64,
        entry: settings::Agent,
        cwd: PathBuf,
        seed: Option<String>,
        cx: &mut Context<Cydonia>,
    ) -> Self {
        let spawn_entry = entry.clone();
        let conn = acp::runtime()
            .spawn(async move { Session::spawn(&spawn_entry, acp::Launch::new(cwd)).await });

        let pump = cx.spawn(async move |this, cx| {
            let opened = conn
                .await
                .unwrap_or_else(|e| Err(anyhow!("the connection task panicked: {e}")));
            let (session, mut events) = match opened {
                Ok(pair) => pair,
                Err(e) => {
                    let _ = this.update(cx, |app, cx| {
                        app.with_session(id, cx, |chat| {
                            chat.lost = true;
                            chat.notice(true, &format!("connection failed: {e:#}"));
                        });
                    });
                    return;
                }
            };

            if this
                .update(cx, |app, cx| {
                    app.with_session(id, cx, |chat| chat.session = Some(session));
                    app.session_connected(id, cx);
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
                let streaming = this.update(cx, |app, cx| {
                    app.with_session(id, cx, |chat| {
                        for event in batch {
                            chat.apply(event);
                        }
                    });
                    app.session(id).is_some_and(|chat| chat.streaming)
                });
                match streaming {
                    Ok(true) => cx.background_executor().timer(STREAM_FRAME).await,
                    Ok(false) => {}
                    Err(_) => return,
                }
            }
        });

        Self {
            id,
            entry,
            session: None,
            items: Vec::new(),
            plan: Vec::new(),
            permission: None,
            commands: Vec::new(),
            title: String::new(),
            streaming: false,
            lost: false,
            queue: VecDeque::new(),
            seed,
            transcript: transcript::State::new(Painter::of(cx)),
            _pump: pump,
        }
    }

    /// Send now, or queue when a turn is in flight.
    pub fn send(&mut self, content: String) {
        if self.streaming {
            self.queue.push_back(content);
        } else {
            self.prompt(content);
        }
    }

    fn prompt(&mut self, content: String) {
        let Some(session) = &self.session else {
            self.notice(false, "not connected yet");
            return;
        };
        session.prompt(&content);
        self.items.push(ChatItem::User(content));
        self.streaming = true;
    }

    /// Cancel the in-flight turn. A pending permission request MUST be
    /// answered `Cancelled` per spec before `session/cancel` goes out.
    pub fn cancel(&mut self) {
        if let Some(prompt) = self.permission.take() {
            prompt.reply.send(RequestPermissionResponse::cancelled());
        }
        if let Some(session) = &self.session
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
                if let Some(next) = self.queue.pop_front() {
                    self.prompt(next);
                }
            }
            Event::Closed => {
                self.lost = true;
                self.streaming = false;
                self.fail_running_tools();
                self.notice(true, "agent connection lost");
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

    fn notice(&mut self, failed: bool, text: &str) {
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
