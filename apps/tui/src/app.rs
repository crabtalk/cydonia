//! Full-screen chat over one ACP session.

use crate::{
    chat::{ChatEntry, PlanRow, PlanStatus, S_DIM, ToolStatus},
    input::{History, InputAction, InputState},
    render::MarkdownRenderer,
    tui,
};
use anyhow::Result;
use crossterm::event::{Event, EventStream, KeyCode, KeyEvent, KeyModifiers};
use cydonia_core::acp::Responder;
use cydonia_core::acp::schema::MaybeUndefined;
use cydonia_core::acp::schema::v1::{
    ContentBlock, InitializeResponse, NewSessionResponse, PlanEntryStatus,
    RequestPermissionOutcome, RequestPermissionRequest, RequestPermissionResponse,
    SelectedPermissionOutcome, SessionUpdate, StopReason, ToolCallContent, ToolCallStatus,
};
use cydonia_core::session::{self, Events, Session};
use cydonia_core::settings;
use futures_util::StreamExt;
use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
};
use std::{collections::VecDeque, time::Duration};
use tokio::time::MissedTickBehavior;

/// Spawn the agent, run the chat, tear down.
pub async fn run(entry: settings::Agent) -> Result<()> {
    // The selector has left the alternate screen and the chat only enters
    // it once the session is up. Say what's happening in between — a first
    // npx run downloads the adapter and looks like a hang otherwise.
    println!(
        "⏺ starting {} — the first run may download the agent…",
        entry.name
    );

    let name = entry.name.clone();
    let cwd = std::env::current_dir().unwrap_or_else(|_| "/".into());
    Session::spawn(&entry, cwd, async |session, events| {
        chat(session, events, &name).await
    })
    .await
}

async fn chat(session: Session, events: Events, agent_name: &str) -> Result<()> {
    let history_path = settings::history_path();
    let mut history = History::new();
    if let Some(ref path) = history_path {
        history.load(path);
    }

    let mut app = App {
        renderer: MarkdownRenderer::new(),
        input: InputState::new(history, Vec::new()),
        scroll: 0,
        message_queue: VecDeque::new(),
        agent: agent_name.to_owned(),
        chat_title: String::new(),
        dirty: true,
        frame_count: 0,
        streaming: false,
        permission: None,
        header: header_lines(agent_name, &session.init, &session.response, &session.cwd),
    };
    app.renderer
        .buffer
        .push(ChatEntry::Text(app.header.clone()));

    let mut terminal = tui::setup()?;
    let result = event_loop(&mut terminal, &mut app, &session, events).await;
    tui::teardown(&mut terminal)?;

    // Never leave the agent hanging on an unanswered permission request.
    if let Some(prompt) = app.permission.take() {
        let _ = prompt.responder.respond(RequestPermissionResponse::new(
            RequestPermissionOutcome::Cancelled,
        ));
    }

    if let Some(ref path) = history_path {
        app.input.history.save(path);
    }
    result
}

// ── App state ────────────────────────────────────────────────────

struct App {
    renderer: MarkdownRenderer,
    input: InputState,
    scroll: usize,
    message_queue: VecDeque<String>,
    agent: String,
    chat_title: String,
    dirty: bool,
    frame_count: u64,
    streaming: bool,
    /// Pending `session/request_permission` shown as a modal.
    permission: Option<PermissionPrompt>,
    /// Connection header shown at the top and after /clear.
    header: Vec<Line<'static>>,
}

struct PermissionPrompt {
    title: String,
    options: Vec<(String, String)>,
    selected: usize,
    responder: Responder<RequestPermissionResponse>,
}

// ── Event loop ───────────────────────────────────────────────────

async fn event_loop(
    terminal: &mut ratatui::Terminal<ratatui::backend::CrosstermBackend<std::io::Stdout>>,
    app: &mut App,
    session: &Session,
    mut events: Events,
) -> Result<()> {
    let mut keys = EventStream::new();
    let mut tick = tokio::time::interval(Duration::from_millis(33));
    tick.set_missed_tick_behavior(MissedTickBehavior::Skip);

    loop {
        if app.dirty {
            let width = terminal.size()?.width as usize;
            app.renderer.set_width(width.saturating_sub(2));
            terminal.draw(|f| draw(f, app))?;
            app.dirty = false;
        }

        tokio::select! {
            // Branch 1: everything from the agent, in wire order.
            event = events.next() => {
                match event {
                    Some(session::Event::Update(update)) => handle_update(update, app),
                    Some(session::Event::Permission(req, responder)) => {
                        open_permission(req, responder, app);
                    }
                    Some(session::Event::TurnDone(result)) => {
                        match result {
                            Ok(reason) => finish_turn(app, reason),
                            Err(e) => {
                                app.renderer.finish();
                                app.streaming = false;
                                app.renderer.buffer.fail_running_tools();
                                push_error(app, &format!("turn failed: {}", session::error_text(&e)));
                            }
                        }
                        if let Some(next) = app.message_queue.pop_front() {
                            send_prompt(app, session, &next);
                        }
                    }
                    None => {
                        app.renderer.finish();
                        app.streaming = false;
                        push_error(app, "agent connection lost");
                    }
                }
                app.dirty = true;
            }

            // Branch 2: terminal events.
            event = keys.next() => {
                match event {
                    Some(Ok(Event::Key(key))) => {
                        if handle_key(key, app, session)? {
                            return Ok(());
                        }
                    }
                    Some(Ok(Event::Resize(_, _))) => app.dirty = true,
                    Some(Err(_)) | None => return Ok(()),
                    _ => {}
                }
            }

            // Branch 3: render tick (spinner animation).
            _ = tick.tick() => {
                app.frame_count += 1;
                if app.renderer.waiting || app.streaming {
                    app.dirty = true;
                }
            }
        }
    }
}

/// Returns `true` when the app should exit.
fn handle_key(key: KeyEvent, app: &mut App, session: &Session) -> Result<bool> {
    // Permission modal intercepts all keys when active.
    if app.permission.is_some() {
        handle_permission_key(key, app, session);
        app.dirty = true;
        return Ok(false);
    }

    // Scrolling.
    if key.code == KeyCode::PageUp {
        let chat_lines = app.renderer.buffer.lines(app.frame_count).len();
        app.scroll = app
            .scroll
            .saturating_add(10)
            .min(chat_lines.saturating_sub(1));
        app.dirty = true;
        return Ok(false);
    }
    if key.code == KeyCode::PageDown {
        app.scroll = app.scroll.saturating_sub(10);
        app.dirty = true;
        return Ok(false);
    }

    // Ctrl+C during a turn: cancel it.
    if key.modifiers.contains(KeyModifiers::CONTROL)
        && key.code == KeyCode::Char('c')
        && app.streaming
    {
        cancel_turn(app, session);
        app.dirty = true;
        return Ok(false);
    }

    match app.input.handle_key(key) {
        InputAction::Submit(content) => {
            if content.is_empty() {
                app.dirty = true;
                return Ok(false);
            }
            app.renderer.buffer.push(ChatEntry::Text(vec![
                Line::raw(""),
                Line::from(Span::styled(
                    format!(" {content} "),
                    Style::new().bg(Color::Indexed(236)),
                )),
                Line::raw(""),
            ]));
            app.scroll = 0;

            match content.as_str() {
                "/exit" => return Ok(true),
                "/clear" => {
                    app.renderer = MarkdownRenderer::new();
                    app.renderer
                        .buffer
                        .push(ChatEntry::Text(app.header.clone()));
                }
                "/help" => push_help(app),
                _ => send_or_queue(app, session, content),
            }
            app.dirty = true;
        }
        InputAction::Interrupt => app.dirty = true,
        InputAction::Eof => {
            if !app.streaming {
                return Ok(true);
            }
        }
        InputAction::Noop => app.dirty = true,
    }
    Ok(false)
}

fn handle_permission_key(key: KeyEvent, app: &mut App, session: &Session) {
    let Some(prompt) = app.permission.as_mut() else {
        return;
    };
    match key.code {
        KeyCode::Up | KeyCode::Char('k') => prompt.selected = prompt.selected.saturating_sub(1),
        KeyCode::Down | KeyCode::Char('j') => {
            prompt.selected = (prompt.selected + 1).min(prompt.options.len().saturating_sub(1));
        }
        KeyCode::Enter => {
            let prompt = app.permission.take().expect("checked above");
            let option_id = prompt.options[prompt.selected].0.clone();
            let _ = prompt.responder.respond(RequestPermissionResponse::new(
                RequestPermissionOutcome::Selected(SelectedPermissionOutcome::new(option_id)),
            ));
        }
        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            cancel_turn(app, session);
        }
        _ => {}
    }
}

// ── Turn management ──────────────────────────────────────────────

fn send_or_queue(app: &mut App, session: &Session, content: String) {
    if app.streaming {
        app.message_queue.push_back(content.clone());
        app.renderer
            .buffer
            .push(ChatEntry::Text(vec![Line::from(Span::styled(
                format!("  [queued] {content}"),
                Style::new().add_modifier(Modifier::DIM),
            ))]));
    } else {
        send_prompt(app, session, &content);
    }
}

fn send_prompt(app: &mut App, session: &Session, content: &str) {
    match session.prompt(content) {
        Ok(()) => {
            app.streaming = true;
            app.renderer.start_waiting();
        }
        Err(e) => push_error(app, &format!("prompt failed: {e}")),
    }
}

/// Cancel the in-flight turn: notify the agent and settle pending state.
/// The turn stays "streaming" until the agent acks with `StopReason`.
fn cancel_turn(app: &mut App, session: &Session) {
    // A pending permission request MUST be answered Cancelled per spec.
    if let Some(prompt) = app.permission.take() {
        let _ = prompt.responder.respond(RequestPermissionResponse::new(
            RequestPermissionOutcome::Cancelled,
        ));
    }
    if let Err(e) = session.cancel() {
        push_error(app, &format!("cancel failed: {e}"));
    }
}

fn finish_turn(app: &mut App, reason: StopReason) {
    app.renderer.finish();
    app.streaming = false;
    app.scroll = 0;
    match reason {
        StopReason::EndTurn => {}
        StopReason::Cancelled => {
            app.renderer.buffer.fail_running_tools();
            push_notice(app, "cancelled");
        }
        StopReason::Refusal => push_notice(app, "the agent refused to continue"),
        StopReason::MaxTokens => push_notice(app, "stopped: max tokens"),
        StopReason::MaxTurnRequests => push_notice(app, "stopped: max turn requests"),
        other => push_notice(app, &format!("stopped: {other:?}")),
    }
}

// ── Incoming events ──────────────────────────────────────────────

fn open_permission(
    req: RequestPermissionRequest,
    responder: Responder<RequestPermissionResponse>,
    app: &mut App,
) {
    let options: Vec<(String, String)> = req
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
    let title = req
        .tool_call
        .fields
        .title
        .clone()
        .unwrap_or_else(|| "Permission required".to_owned());
    app.renderer.finish();
    app.permission = Some(PermissionPrompt {
        title,
        options,
        selected: 0,
        responder,
    });
}

fn handle_update(update: SessionUpdate, app: &mut App) {
    match update {
        SessionUpdate::AgentMessageChunk(chunk) => {
            app.renderer.push_text(&content_text(&chunk.content));
        }
        SessionUpdate::AgentThoughtChunk(chunk) => {
            app.renderer.push_thinking(&content_text(&chunk.content));
        }
        SessionUpdate::ToolCall(call) => {
            app.renderer
                .push_tool_call(&call.tool_call_id.to_string(), call.title);
            let id = call.tool_call_id.to_string();
            apply_tool_status(app, &id, call.status);
            push_tool_content(
                app,
                &id,
                &call.content,
                call.status == ToolCallStatus::Failed,
            );
        }
        SessionUpdate::ToolCallUpdate(update) => {
            let id = update.tool_call_id.to_string();
            if let Some(title) = update.fields.title {
                app.renderer.set_tool_label(&id, title);
            }
            let failed = update.fields.status == Some(ToolCallStatus::Failed);
            if let Some(content) = update.fields.content {
                push_tool_content(app, &id, &content, failed);
            }
            if let Some(status) = update.fields.status {
                apply_tool_status(app, &id, status);
            }
        }
        SessionUpdate::SessionInfoUpdate(info) => match info.title {
            MaybeUndefined::Value(title) => app.chat_title = title,
            MaybeUndefined::Null => app.chat_title.clear(),
            MaybeUndefined::Undefined => {}
        },
        SessionUpdate::AvailableCommandsUpdate(cmds) => {
            app.input.extra_commands = cmds
                .available_commands
                .into_iter()
                .map(|c| c.name)
                .collect();
        }
        SessionUpdate::Plan(plan) => {
            let rows = plan
                .entries
                .into_iter()
                .map(|entry| PlanRow {
                    text: entry.content,
                    status: match entry.status {
                        PlanEntryStatus::Completed => PlanStatus::Done,
                        PlanEntryStatus::InProgress => PlanStatus::Active,
                        _ => PlanStatus::Pending,
                    },
                })
                .collect();
            app.renderer.buffer.set_plan(rows);
        }
        // We echo the user's message locally.
        SessionUpdate::UserMessageChunk(_) => {}
        _ => {}
    }
    app.scroll = 0;
}

fn apply_tool_status(app: &mut App, id: &str, status: ToolCallStatus) {
    let mapped = match status {
        ToolCallStatus::Completed => ToolStatus::Success,
        ToolCallStatus::Failed => ToolStatus::Failure,
        _ => ToolStatus::Running,
    };
    app.renderer.set_tool_status(id, mapped);
}

fn push_tool_content(app: &mut App, id: &str, content: &[ToolCallContent], failed: bool) {
    let text: String = content
        .iter()
        .map(|c| match c {
            ToolCallContent::Content(inner) => content_text(&inner.content),
            ToolCallContent::Diff(diff) => format!("edited {}", diff.path.display()),
            ToolCallContent::Terminal(_) => "[terminal]".to_owned(),
            _ => "[content]".to_owned(),
        })
        .collect::<Vec<_>>()
        .join("\n");
    if !text.is_empty() {
        app.renderer.push_tool_result(id, &text, failed);
    }
}

fn content_text(block: &ContentBlock) -> String {
    match block {
        ContentBlock::Text(text) => text.text.clone(),
        _ => "[non-text content]".to_owned(),
    }
}

// ── Chat helpers ─────────────────────────────────────────────────

/// Connection header: who we're talking to, over what, and where.
fn header_lines(
    agent_name: &str,
    init: &InitializeResponse,
    session: &NewSessionResponse,
    cwd: &std::path::Path,
) -> Vec<Line<'static>> {
    let key = |s: &str| Span::styled(format!("{s:<7}"), S_DIM);
    let value = |s: String| Span::raw(s);

    let mut rows = vec![Line::from(Span::styled(
        format!("Cydonia — {agent_name} — Ctrl+D to exit, /help for keys"),
        Style::new()
            .fg(Color::Indexed(173))
            .add_modifier(Modifier::BOLD),
    ))];

    let protocol = format!("v{}", init.protocol_version.as_u16());
    let agent = match &init.agent_info {
        Some(info) => {
            let name = info.title.clone().unwrap_or_else(|| info.name.clone());
            format!("{name} {} · acp {protocol}", info.version)
        }
        None => format!("acp {protocol}"),
    };
    rows.push(Line::from(vec![key("agent"), value(agent)]));

    if let Some(modes) = &session.modes {
        let available = modes
            .available_modes
            .iter()
            .map(|m| m.name.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        let mut mode = modes.current_mode_id.to_string();
        if !available.is_empty() {
            mode.push_str(&format!(" ({available})"));
        }
        rows.push(Line::from(vec![key("mode"), value(mode)]));
    }

    let cwd = match dirs::home_dir().and_then(|home| cwd.strip_prefix(home).ok()) {
        Some(rel) => format!("~/{}", rel.display()),
        None => cwd.display().to_string(),
    };
    rows.push(Line::from(vec![key("cwd"), value(cwd)]));

    let mut lines = boxed(rows);
    lines.push(Line::raw(""));
    lines
}

/// Wrap rows in a rounded box drawn with text (the chat area is one flat
/// paragraph, so a ratatui `Block` widget can't scroll with the content).
fn boxed(rows: Vec<Line<'static>>) -> Vec<Line<'static>> {
    let width =
        |line: &Line| -> usize { line.spans.iter().map(|s| s.content.chars().count()).sum() };
    let inner = rows.iter().map(width).max().unwrap_or(0);
    let border = S_DIM;

    let mut out = vec![Line::from(Span::styled(
        format!(" ╭{}╮", "─".repeat(inner + 2)),
        border,
    ))];
    for row in rows {
        let pad = inner - width(&row);
        let mut spans = vec![Span::styled(" │ ".to_owned(), border)];
        spans.extend(row.spans);
        spans.push(Span::raw(" ".repeat(pad)));
        spans.push(Span::styled(" │".to_owned(), border));
        out.push(Line::from(spans));
    }
    out.push(Line::from(Span::styled(
        format!(" ╰{}╯", "─".repeat(inner + 2)),
        border,
    )));
    out
}

fn push_error(app: &mut App, message: &str) {
    app.renderer
        .buffer
        .push(ChatEntry::Text(vec![Line::from(Span::styled(
            format!("  {message}"),
            Style::new().fg(Color::Indexed(204)),
        ))]));
}

fn push_notice(app: &mut App, message: &str) {
    app.renderer
        .buffer
        .push(ChatEntry::Text(vec![Line::from(Span::styled(
            format!("  [{message}]"),
            Style::new().add_modifier(Modifier::DIM),
        ))]));
}

fn push_help(app: &mut App) {
    let lines = [
        "  /clear — clear the transcript",
        "  /exit  — quit (also Ctrl+D)",
        "  Ctrl+C — cancel the current turn",
        "  Shift+Enter — newline · PageUp/PageDown — scroll",
    ]
    .iter()
    .map(|s| {
        Line::from(Span::styled(
            s.to_string(),
            Style::new().add_modifier(Modifier::DIM),
        ))
    })
    .collect();
    app.renderer.buffer.push(ChatEntry::Text(lines));
}

// ── Drawing ──────────────────────────────────────────────────────

fn draw(frame: &mut ratatui::Frame, app: &App) {
    let input_height = app.input.height().min(frame.area().height / 3).max(3);
    let chunks = Layout::vertical([Constraint::Min(1), Constraint::Length(input_height)])
        .split(frame.area());

    draw_chat(frame, chunks[0], app);
    app.input
        .render(frame, chunks[1], &app.agent, &app.chat_title);

    if let Some(ref prompt) = app.permission {
        draw_permission(frame, prompt);
    }
}

fn draw_chat(frame: &mut ratatui::Frame, area: Rect, app: &App) {
    let mut lines = app.renderer.buffer.lines(app.frame_count);
    if let Some(current) = app.renderer.current_line() {
        lines.push(current);
    }
    if app.renderer.waiting {
        let spinner = if app.frame_count % 30 < 15 {
            "⏺"
        } else {
            " "
        };
        lines.push(Line::from(Span::styled(
            spinner,
            Style::new().add_modifier(Modifier::DIM),
        )));
    }

    let total_lines = lines.len() as u16;
    let visible = area.height;
    let max_scroll = total_lines.saturating_sub(visible);
    let scroll_offset = if app.scroll == 0 {
        max_scroll
    } else {
        max_scroll.saturating_sub(app.scroll as u16)
    };

    frame.render_widget(Paragraph::new(lines).scroll((scroll_offset, 0)), area);
}

fn draw_permission(frame: &mut ratatui::Frame, prompt: &PermissionPrompt) {
    let area = frame.area();
    let height = (prompt.options.len() as u16 + 4).min(area.height);
    let width = area.width.saturating_sub(8).min(70);
    let rect = Rect::new(
        area.width.saturating_sub(width) / 2,
        area.height.saturating_sub(height) / 2,
        width,
        height,
    );

    let mut lines = Vec::new();
    for (i, (_, name)) in prompt.options.iter().enumerate() {
        let (marker, style) = if i == prompt.selected {
            (
                "> ",
                Style::new()
                    .fg(Color::Rgb(215, 119, 87))
                    .add_modifier(Modifier::BOLD),
            )
        } else {
            ("  ", Style::new().fg(Color::DarkGray))
        };
        lines.push(Line::from(vec![
            Span::styled(marker, style),
            Span::styled(name.clone(), style),
        ]));
    }
    lines.push(Line::raw(""));
    lines.push(Line::from(Span::styled(
        "↑/↓ move · Enter select · Ctrl+C cancel turn",
        Style::new().add_modifier(Modifier::DIM),
    )));

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(tui::border_focused())
        .title(format!(" {} ", prompt.title));
    frame.render_widget(Clear, rect);
    frame.render_widget(Paragraph::new(lines).block(block), rect);
}
