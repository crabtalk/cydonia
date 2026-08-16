//! One live agent session bridged into the UI.
//!
//! The connection future runs on the background executor; the scoped
//! `Session::spawn` closure hands `(Session, Events)` to the UI over a
//! oneshot and then parks on a shutdown signal, so dropping `ChatSession`
//! tears the connection (and the agent process) down. A foreground pump
//! drains the core event channel in coalesced batches with a 120ms frame
//! floor while streaming — one notify per frame, not per chunk.

use crate::app::Cydonia;
use cydonia_core::{
    acp::{
        Responder,
        schema::{
            MaybeUndefined,
            v1::{
                ContentBlock, PlanEntryStatus, RequestPermissionOutcome, RequestPermissionRequest,
                RequestPermissionResponse, SelectedPermissionOutcome, SessionUpdate, StopReason,
                ToolCallContent, ToolCallStatus,
            },
        },
    },
    session::{self, Event, Events, Session},
    settings,
};
use futures::{FutureExt, StreamExt, channel::oneshot};
use gpui::{Context, ListAlignment, ListState, Task, px};
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
        label: String,
        status: ToolStatus,
        output: String,
    },
    Notice(String),
}

pub struct PermissionPrompt {
    pub title: String,
    /// `(option_id, name)` pairs.
    pub options: Vec<(String, String)>,
    responder: Responder<RequestPermissionResponse>,
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
    pub list: ListState,
    /// The user scrolled away from the pinned bottom.
    pub scrolled: bool,
    /// Items currently reflected in `list`.
    listed: usize,
    /// Lowest item index mutated since the last `sync_list`.
    dirty_from: Option<usize>,
    _shutdown: oneshot::Sender<()>,
    _pump: Task<()>,
}

impl ChatSession {
    pub fn connect(
        id: u64,
        entry: settings::Agent,
        cwd: PathBuf,
        cx: &mut Context<Cydonia>,
    ) -> Self {
        let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();
        let (ready_tx, ready_rx) = oneshot::channel::<(Session, Events)>();

        let spawn_entry = entry.clone();
        let conn = cx.background_executor().spawn(async move {
            Session::spawn(&spawn_entry, cwd, None, async |session, events| {
                let _ = ready_tx.send((session, events));
                let _ = shutdown_rx.await;
                Ok(())
            })
            .await
        });

        let pump = cx.spawn(async move |this, cx| {
            let mut conn = conn.fuse();
            let (session, mut events) = match ready_rx.await {
                Ok(pair) => pair,
                Err(_) => {
                    let error = match conn.await {
                        Ok(()) => "agent exited before the session opened".to_owned(),
                        Err(e) => format!("{e:#}"),
                    };
                    let _ = this.update(cx, |app, cx| {
                        app.with_session(id, cx, |chat| {
                            chat.lost = true;
                            chat.notice(&format!("connection failed: {error}"));
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

            loop {
                let event = futures::select! {
                    event = events.next() => match event {
                        Some(event) => event,
                        None => break,
                    },
                    _ = conn => break,
                };
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

            let _ = this.update(cx, |app, cx| {
                app.with_session(id, cx, |chat| {
                    chat.lost = true;
                    chat.streaming = false;
                    chat.fail_running_tools();
                    chat.notice("agent connection lost");
                });
            });
        });

        let list = ListState::new(0, ListAlignment::Bottom, px(512.));
        let weak = cx.entity().downgrade();
        list.set_scroll_handler(move |event, _, cx| {
            let scrolled = event.is_scrolled;
            let _ = weak.update(cx, |app, cx| app.set_scrolled(id, scrolled, cx));
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
            list,
            scrolled: false,
            listed: 0,
            dirty_from: None,
            _shutdown: shutdown_tx,
            _pump: pump,
        }
    }

    /// Reflect item mutations into the list: one splice covering
    /// everything from the lowest touched index (remeasure) through the
    /// appended tail. A pinned-bottom list stays pinned across splices.
    pub fn sync_list(&mut self) {
        let len = self.items.len();
        let from = self.dirty_from.take().unwrap_or(len).min(self.listed);
        if from < self.listed || len != self.listed {
            self.list.splice(from..self.listed, len - from);
            self.listed = len;
        }
    }

    fn touch(&mut self, ix: usize) {
        self.dirty_from = Some(self.dirty_from.map_or(ix, |d| d.min(ix)));
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
            self.notice("not connected yet");
            return;
        };
        match session.prompt(&content) {
            Ok(()) => {
                self.items.push(ChatItem::User(content));
                self.touch(self.items.len() - 1);
                self.streaming = true;
            }
            Err(e) => self.notice(&format!("prompt failed: {}", session::error_text(&e))),
        }
    }

    /// Cancel the in-flight turn. A pending permission request MUST be
    /// answered `Cancelled` per spec before `session/cancel` goes out.
    pub fn cancel(&mut self) {
        if let Some(prompt) = self.permission.take() {
            let _ = prompt.responder.respond(RequestPermissionResponse::new(
                RequestPermissionOutcome::Cancelled,
            ));
        }
        if let Some(session) = &self.session
            && let Err(e) = session.cancel()
        {
            self.notice(&format!("cancel failed: {}", session::error_text(&e)));
        }
    }

    /// Answer the pending permission prompt with the chosen option id.
    pub fn respond_permission(&mut self, option_id: String) {
        if let Some(prompt) = self.permission.take() {
            let _ = prompt.responder.respond(RequestPermissionResponse::new(
                RequestPermissionOutcome::Selected(SelectedPermissionOutcome::new(option_id)),
            ));
        }
    }

    fn apply(&mut self, event: Event) {
        match event {
            Event::Update(update) => self.apply_update(update),
            Event::Permission(request, responder) => self.open_permission(request, responder),
            Event::TurnDone(result) => {
                self.finish_thinking();
                self.streaming = false;
                match result {
                    Ok(StopReason::EndTurn) => {}
                    Ok(StopReason::Cancelled) => {
                        self.fail_running_tools();
                        self.notice("cancelled");
                    }
                    Ok(StopReason::Refusal) => self.notice("the agent refused to continue"),
                    Ok(StopReason::MaxTokens) => self.notice("stopped: max tokens"),
                    Ok(StopReason::MaxTurnRequests) => self.notice("stopped: max turn requests"),
                    Ok(other) => self.notice(&format!("stopped: {other:?}")),
                    Err(e) => {
                        self.fail_running_tools();
                        self.notice(&format!("turn failed: {}", session::error_text(&e)));
                    }
                }
                if let Some(next) = self.queue.pop_front() {
                    self.prompt(next);
                }
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
                self.touch(self.items.len() - 1);
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
                self.touch(self.items.len() - 1);
            }
            SessionUpdate::ToolCall(call) => {
                self.finish_thinking();
                self.items.push(ChatItem::Tool {
                    id: call.tool_call_id.to_string(),
                    label: call.title,
                    status: tool_status(call.status),
                    output: tool_content_text(&call.content),
                });
                self.touch(self.items.len() - 1);
            }
            SessionUpdate::ToolCallUpdate(update) => {
                let id = update.tool_call_id.to_string();
                let Some(ix) = self.items.iter().rposition(
                    |item| matches!(item, ChatItem::Tool { id: tool, .. } if *tool == id),
                ) else {
                    return;
                };
                let ChatItem::Tool {
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
                self.touch(ix);
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
        responder: Responder<RequestPermissionResponse>,
    ) {
        let options: Vec<(String, String)> = request
            .options
            .into_iter()
            .map(|opt| (opt.option_id.to_string(), opt.name))
            .collect();
        if options.is_empty() {
            let _ = responder.respond(RequestPermissionResponse::new(
                RequestPermissionOutcome::Cancelled,
            ));
            return;
        }
        // A replaced prompt must still be answered — an unanswered
        // responder hangs the agent.
        if let Some(previous) = self.permission.take() {
            let _ = previous.responder.respond(RequestPermissionResponse::new(
                RequestPermissionOutcome::Cancelled,
            ));
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
            responder,
        });
    }

    fn notice(&mut self, text: &str) {
        self.items.push(ChatItem::Notice(text.to_owned()));
        self.touch(self.items.len() - 1);
    }

    fn finish_thinking(&mut self) {
        let last = self.items.len().saturating_sub(1);
        if let Some(ChatItem::Thinking { done, .. }) = self.items.last_mut()
            && !*done
        {
            *done = true;
            self.touch(last);
        }
    }

    fn fail_running_tools(&mut self) {
        for ix in 0..self.items.len() {
            if let ChatItem::Tool { status, .. } = &mut self.items[ix]
                && *status == ToolStatus::Running
            {
                *status = ToolStatus::Failure;
                self.touch(ix);
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
            ToolCallContent::Content(inner) => content_text(&inner.content),
            ToolCallContent::Diff(diff) => format!("edited {}", diff.path.display()),
            ToolCallContent::Terminal(_) => "[terminal]".to_owned(),
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
