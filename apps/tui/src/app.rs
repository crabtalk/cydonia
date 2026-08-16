//! Full-screen chat over one ACP session.

use crate::{
    chat::{ChatBuffer, ChatEntry, PlanRow, PlanStatus, S_DIM, ToolStatus},
    input::{History, InputAction, InputState},
    mcp::{McpAction, McpEvent, McpPicker},
    tui,
};
use anyhow::Result;
use crossterm::{
    event::{Event, EventStream, KeyCode, KeyEvent, KeyModifiers},
    execute,
    terminal::{BeginSynchronizedUpdate, EndSynchronizedUpdate, SetTitle},
};
use cydonia_core::acp::Responder;
use cydonia_core::acp::schema::MaybeUndefined;
use cydonia_core::acp::schema::v1::{
    ContentBlock, EmbeddedResource, EmbeddedResourceResource, InitializeResponse,
    NewSessionResponse, PlanEntryStatus, RequestPermissionOutcome, RequestPermissionRequest,
    RequestPermissionResponse, SelectedPermissionOutcome, SessionConfigKind, SessionConfigOption,
    SessionConfigOptionValue, SessionConfigSelect, SessionConfigSelectOption,
    SessionConfigSelectOptions, SessionUpdate, StopReason, TextResourceContents, ToolCallContent,
    ToolCallLocation, ToolCallStatus, ToolKind,
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
use tokio::time::{Instant, sleep_until};

/// Frame-rate ceiling: coalesced frame requests never draw more often
/// than this (codex's TARGET_FRAME_INTERVAL).
const MIN_FRAME_INTERVAL: Duration = Duration::from_millis(8);
/// Spinner cadence while something animates on screen.
const ANIMATION_FRAME: Duration = Duration::from_millis(80);
/// Pastes at or above these thresholds collapse to a placeholder.
const LARGE_PASTE_LINES: usize = 3;
const LARGE_PASTE_CHARS: usize = 150;
/// How long "press Ctrl+C again to quit" stays armed.
const QUIT_HINT_TTL: Duration = Duration::from_secs(5);
/// Terminal-title length cap.
const MAX_TITLE_CHARS: usize = 240;

/// Spawn the agent, run the chat, tear down. `previous` loads that
/// session's history instead of starting fresh.
pub async fn run(entry: settings::Agent, previous: Option<String>) -> Result<()> {
    // The selector has left the alternate screen and the chat only enters
    // it once the session is up. Say what's happening in between — a first
    // npx run downloads the adapter and looks like a hang otherwise.
    println!(
        "⏺ starting {} — the first run may download the agent…",
        entry.name
    );

    let name = entry.name.clone();
    let launch = session::Launch {
        cwd: std::env::current_dir().unwrap_or_else(|_| "/".into()),
        previous,
        status: Some(Box::new(|message| println!("⏺ {message}"))),
    };
    Session::spawn(&entry, launch, async |session, events| {
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

    let (modes, current_mode) = match &session.response.modes {
        Some(state) => (
            state
                .available_modes
                .iter()
                .map(|m| (m.id.to_string(), m.name.clone()))
                .collect(),
            Some(state.current_mode_id.to_string()),
        ),
        None => (Vec::new(), None),
    };

    let mut input = InputState::new(history, Vec::new());
    input.set_files(workspace_files(&session.cwd));

    let mut app = App {
        buffer: ChatBuffer::new(),
        input,
        scroll: usize::MAX,
        last_scroll_top: 0,
        last_max_scroll: 0,
        message_queue: VecDeque::new(),
        agent: agent_name.to_owned(),
        chat_title: String::new(),
        frame_count: 0,
        streaming: false,
        permission: None,
        header: header_lines(agent_name, &session.init, &session.response, &session.cwd),
        pending_pastes: Vec::new(),
        paste_seq: 0,
        quit_hint: None,
        last_title: String::new(),
        modes,
        current_mode,
        usage: None,
        config: session.response.config_options.clone().unwrap_or_default(),
        embed_context: session
            .init
            .agent_capabilities
            .prompt_capabilities
            .embedded_context,
        mcp: None,
    };
    app.buffer.push(ChatEntry::Text(app.header.clone()));
    if session.loaded {
        push_notice(&mut app, "continuing previous session");
    }
    settings::remember_session(agent_name, &session.cwd, &session.session_id.to_string());

    let mut terminal = tui::setup()?;
    let result = event_loop(&mut terminal, &mut app, &session, events).await;
    tui::teardown(&mut terminal)?;
    let _ = execute!(std::io::stdout(), SetTitle(""));

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
    buffer: ChatBuffer,
    input: InputState,
    /// Scroll top offset into the transcript; `usize::MAX` is the
    /// follow-bottom sentinel (clamped at render).
    scroll: usize,
    /// Effective scroll top of the last draw (for paging).
    last_scroll_top: u16,
    /// Max scroll top of the last draw (for re-arming follow).
    last_max_scroll: u16,
    message_queue: VecDeque<String>,
    agent: String,
    chat_title: String,
    frame_count: u64,
    streaming: bool,
    /// Pending `session/request_permission` shown as a modal.
    permission: Option<PermissionPrompt>,
    /// Connection header shown at the top and after /clear.
    header: Vec<Line<'static>>,
    /// Collapsed pastes: `(placeholder, real text)`, expanded on submit.
    pending_pastes: Vec<(String, String)>,
    paste_seq: usize,
    /// Expiry of the "press Ctrl+C again to quit" hint.
    quit_hint: Option<Instant>,
    /// Last terminal title written (dedupes OSC writes).
    last_title: String,
    /// Session modes advertised at `session/new`: `(id, name)`.
    modes: Vec<(String, String)>,
    /// Current mode id (updated by `/mode` and `CurrentModeUpdate`).
    current_mode: Option<String>,
    /// Latest context/cost figures from `UsageUpdate`.
    usage: Option<Usage>,
    /// Session config options (model etc.), replaced wholesale by
    /// `ConfigOptionUpdate` snapshots.
    config: Vec<SessionConfigOption>,
    /// Whether the agent accepts embedded resources in prompts
    /// (`PromptCapabilities.embedded_context`).
    embed_context: bool,
    /// The `/mcp` modal, when open.
    mcp: Option<McpPicker>,
}

struct Usage {
    used: u64,
    size: u64,
    cost: Option<(f64, String)>,
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
    // Registry search and installs run off the loop; results come back
    // here so a slow network never blocks a frame.
    let (mcp_tx, mut mcp_rx) = tokio::sync::mpsc::unbounded_channel::<McpEvent>();
    let data_dir = settings::data_dir().unwrap_or_else(|_| ".".into());
    // Frame scheduling: sources *request* a frame; only the deadline
    // branch draws. N requests inside one interval coalesce into one
    // draw, clamped to MIN_FRAME_INTERVAL since the last one. Idle
    // means no wakeups at all.
    let mut next_frame: Option<Instant> = Some(Instant::now());
    let mut last_draw = Instant::now() - MIN_FRAME_INTERVAL;

    loop {
        let deadline = next_frame.unwrap_or_else(Instant::now);

        tokio::select! {
            // Branch 1: everything from the agent, in wire order —
            // drained as a batch so a burst costs one frame.
            event = events.next() => {
                let Some(event) = event else {
                    app.buffer.finish();
                    app.streaming = false;
                    push_error(app, "agent connection lost");
                    request_frame(&mut next_frame, last_draw);
                    continue;
                };
                handle_agent_event(event, app, session);
                while let Ok(event) = events.try_recv() {
                    handle_agent_event(event, app, session);
                }
                request_frame(&mut next_frame, last_draw);
            }

            // Branch 2: terminal events.
            event = keys.next() => {
                match event {
                    Some(Ok(Event::Key(key))) => {
                        if let Some(picker) = app.mcp.as_mut() {
                            match picker.handle_key(key) {
                                McpAction::None => {}
                                McpAction::Close => {
                                    let dirty = picker.dirty();
                                    app.mcp = None;
                                    if dirty {
                                        push_notice(app, "mcp changes apply to the next session");
                                    }
                                }
                                McpAction::Notice(notice) => push_notice(app, &notice),
                                McpAction::Search(query) => {
                                    let tx = mcp_tx.clone();
                                    tokio::task::spawn_blocking(move || {
                                        let found = cydonia_registry::mcp::search(&query)
                                            .map_err(|e| format!("{e:#}"));
                                        let _ = tx.send(McpEvent::Found(found));
                                    });
                                }
                                McpAction::Add(server) => {
                                    let (tx, dir) = (mcp_tx.clone(), data_dir.clone());
                                    tokio::task::spawn_blocking(move || {
                                        let added = crate::mcp::install(&dir, &server);
                                        let _ = tx.send(McpEvent::Added(added));
                                    });
                                }
                            }
                        } else if handle_key(key, app, session)? {
                            return Ok(());
                        }
                    }
                    Some(Ok(Event::Paste(text))) => handle_paste(&text, app),
                    Some(Ok(Event::Resize(_, _))) => {}
                    Some(Err(_)) | None => return Ok(()),
                    _ => {}
                }
                request_frame(&mut next_frame, last_draw);
            }

            // Branch 3: MCP registry work finishing.
            Some(event) = mcp_rx.recv() => {
                if let Some(picker) = app.mcp.as_mut()
                    && let Some(notice) = picker.apply(event)
                {
                    push_notice(app, &notice);
                }
                request_frame(&mut next_frame, last_draw);
            }

            // Branch 3: the scheduled frame.
            _ = sleep_until(deadline), if next_frame.is_some() => {
                app.frame_count += 1;
                app.buffer.commit_tick();
                if app.quit_hint.is_some_and(|t| t <= Instant::now()) {
                    app.quit_hint = None;
                }
                apply_title(app);
                execute!(std::io::stdout(), BeginSynchronizedUpdate)?;
                terminal.draw(|f| draw(f, app))?;
                execute!(std::io::stdout(), EndSynchronizedUpdate)?;
                last_draw = Instant::now();
                next_frame = if app.buffer.revealing() {
                    Some(last_draw + MIN_FRAME_INTERVAL)
                } else if app.buffer.animating() {
                    Some(last_draw + ANIMATION_FRAME)
                } else {
                    None
                };
                if let Some(expiry) = app.quit_hint {
                    next_frame = Some(next_frame.map_or(expiry, |d| d.min(expiry)));
                }
            }
        }
    }
}

/// Request a frame: keep the earliest pending deadline, never earlier
/// than `MIN_FRAME_INTERVAL` after the previous draw.
fn request_frame(next_frame: &mut Option<Instant>, last_draw: Instant) {
    let at = Instant::now().max(last_draw + MIN_FRAME_INTERVAL);
    *next_frame = Some(next_frame.map_or(at, |d| d.min(at)));
}

fn handle_agent_event(event: session::Event, app: &mut App, session: &Session) {
    match event {
        session::Event::Update(update) => handle_update(update, app),
        session::Event::Permission(req, responder) => {
            open_permission(req, responder, app);
        }
        session::Event::TurnDone(result) => {
            match result {
                Ok(reason) => finish_turn(app, reason),
                Err(e) => {
                    app.buffer.finish();
                    app.streaming = false;
                    app.buffer.fail_running_tools();
                    push_error(app, &format!("turn failed: {}", session::error_text(&e)));
                }
            }
            if let Some(next) = app.message_queue.pop_front() {
                send_prompt(app, session, &next);
            }
        }
    }
}

/// Returns `true` when the app should exit.
fn handle_key(key: KeyEvent, app: &mut App, session: &Session) -> Result<bool> {
    // A second Ctrl+C inside the hint window quits; any other key
    // disarms the hint.
    let quit_armed = app.quit_hint.take().is_some_and(|t| t > Instant::now());

    // Permission modal intercepts all keys when active.
    if app.permission.is_some() {
        handle_permission_key(key, app, session);
        return Ok(false);
    }

    // Scrolling: paging pins the view; paging back to the bottom
    // re-arms follow (`usize::MAX`).
    if key.code == KeyCode::PageUp {
        app.scroll = (app.last_scroll_top.saturating_sub(10)) as usize;
        return Ok(false);
    }
    if key.code == KeyCode::PageDown {
        let top = app.last_scroll_top.saturating_add(10);
        app.scroll = if top >= app.last_max_scroll {
            usize::MAX
        } else {
            top as usize
        };
        return Ok(false);
    }

    // Ctrl+C during a turn: cancel it.
    if key.modifiers.contains(KeyModifiers::CONTROL)
        && key.code == KeyCode::Char('c')
        && app.streaming
    {
        cancel_turn(app, session);
        return Ok(false);
    }

    match app.input.handle_key(key) {
        InputAction::Submit(content) => {
            if content.is_empty() {
                return Ok(false);
            }
            app.buffer.push_user(&content);
            app.scroll = usize::MAX;

            match content.as_str() {
                "/exit" => return Ok(true),
                "/clear" => {
                    app.buffer = ChatBuffer::new();
                    app.buffer.push(ChatEntry::Text(app.header.clone()));
                }
                "/help" => push_help(app),
                _ if content == "/mode" || content.starts_with("/mode ") => {
                    handle_mode_command(app, session, &content);
                }
                _ if content == "/config" || content.starts_with("/config ") => {
                    handle_config_command(app, session, &content);
                }
                "/mcp" => app.mcp = Some(McpPicker::open()),
                _ => {
                    // The echo keeps the collapsed placeholder; the
                    // agent gets the real pasted text.
                    let expanded = expand_pastes(app, &content);
                    send_or_queue(app, session, expanded);
                }
            }
        }
        InputAction::Interrupt => {
            // Idle Ctrl+C: double-press (same key) quits.
            if quit_armed {
                return Ok(true);
            }
            app.quit_hint = Some(Instant::now() + QUIT_HINT_TTL);
        }
        InputAction::Noop => {}
        InputAction::Eof => {
            if !app.streaming {
                return Ok(true);
            }
        }
    }
    Ok(false)
}

/// `/mode` lists the session modes; `/mode <name>` switches (matched
/// by id or name, case-insensitive).
fn handle_mode_command(app: &mut App, session: &Session, content: &str) {
    if app.modes.is_empty() {
        push_notice(app, "this agent has no session modes");
        return;
    }
    let list = |app: &App| {
        app.modes
            .iter()
            .map(|(id, name)| {
                if app.current_mode.as_deref() == Some(id) {
                    format!("{name}*")
                } else {
                    name.clone()
                }
            })
            .collect::<Vec<_>>()
            .join(", ")
    };

    let arg = content.strip_prefix("/mode").unwrap_or_default().trim();
    if arg.is_empty() {
        let modes = list(app);
        push_notice(app, &format!("modes: {modes}"));
        return;
    }
    let wanted = arg.to_lowercase();
    let Some((id, name)) = app
        .modes
        .iter()
        .find(|(id, name)| id.to_lowercase() == wanted || name.to_lowercase() == wanted)
        .cloned()
    else {
        let modes = list(app);
        push_error(app, &format!("unknown mode {arg:?} — available: {modes}"));
        return;
    };
    match session.set_mode(&id) {
        Ok(()) => {
            // Optimistic; a CurrentModeUpdate from the agent overrides.
            app.current_mode = Some(id);
            push_notice(app, &format!("mode → {name}"));
        }
        Err(e) => push_error(
            app,
            &format!("set mode failed: {}", session::error_text(&e)),
        ),
    }
}

/// Flatten grouped select choices into one list.
fn config_choices(select: &SessionConfigSelect) -> Vec<&SessionConfigSelectOption> {
    match &select.options {
        SessionConfigSelectOptions::Ungrouped(options) => options.iter().collect(),
        SessionConfigSelectOptions::Grouped(groups) => {
            groups.iter().flat_map(|g| g.options.iter()).collect()
        }
        _ => Vec::new(),
    }
}

fn describe_config(option: &SessionConfigOption) -> String {
    match &option.kind {
        SessionConfigKind::Select(select) => {
            let choices = config_choices(select);
            let current = choices
                .iter()
                .find(|c| c.value == select.current_value)
                .map(|c| c.name.as_str())
                .unwrap_or("?");
            let names: Vec<&str> = choices.iter().map(|c| c.name.as_str()).collect();
            format!("{}: {} ({})", option.name, current, names.join(", "))
        }
        SessionConfigKind::Boolean(b) => {
            let state = if b.current_value { "on" } else { "off" };
            format!("{}: {} (on, off)", option.name, state)
        }
        _ => format!("{}: (unsupported kind)", option.name),
    }
}

/// `/config` lists the session config options; `/config <option> <value>`
/// sets one (matched by id or name, case-insensitive).
fn handle_config_command(app: &mut App, session: &Session, content: &str) {
    if app.config.is_empty() {
        push_notice(app, "this agent has no config options");
        return;
    }
    let arg = content
        .strip_prefix("/config")
        .unwrap_or_default()
        .trim()
        .to_owned();
    if arg.is_empty() {
        let lines: Vec<String> = app.config.iter().map(describe_config).collect();
        for line in lines {
            push_notice(app, &line);
        }
        return;
    }

    // Option names may contain spaces, so match "<option> <value>" by
    // prefix rather than splitting on the first space.
    let lower = arg.to_lowercase();
    let mut target: Option<(usize, String)> = None;
    'outer: for (ix, option) in app.config.iter().enumerate() {
        for key in [
            option.id.to_string().to_lowercase(),
            option.name.to_lowercase(),
        ] {
            if lower == key {
                target = Some((ix, String::new()));
                break 'outer;
            }
            if let Some(rest) = lower.strip_prefix(&format!("{key} ")) {
                target = Some((ix, rest.trim().to_owned()));
                break 'outer;
            }
        }
    }
    let Some((ix, value)) = target else {
        push_error(app, &format!("unknown config option {arg:?} — try /config"));
        return;
    };
    if value.is_empty() {
        let line = describe_config(&app.config[ix]);
        push_notice(app, &line);
        return;
    }

    let option = &app.config[ix];
    let id = option.id.to_string();
    let option_name = option.name.clone();
    match &option.kind {
        SessionConfigKind::Select(select) => {
            let Some((choice_value, choice_name)) = config_choices(select)
                .into_iter()
                .find(|c| {
                    c.value.to_string().to_lowercase() == value || c.name.to_lowercase() == value
                })
                .map(|c| (c.value.clone(), c.name.clone()))
            else {
                let line = describe_config(&app.config[ix]);
                push_error(app, &format!("unknown value {value:?} — {line}"));
                return;
            };
            match session.set_config_option(
                &id,
                SessionConfigOptionValue::value_id(choice_value.clone()),
            ) {
                Ok(()) => {
                    // Optimistic; a ConfigOptionUpdate overrides.
                    if let SessionConfigKind::Select(select) = &mut app.config[ix].kind {
                        select.current_value = choice_value;
                    }
                    push_notice(app, &format!("{option_name} → {choice_name}"));
                }
                Err(e) => push_error(
                    app,
                    &format!("set config failed: {}", session::error_text(&e)),
                ),
            }
        }
        SessionConfigKind::Boolean(_) => {
            let parsed = match value.as_str() {
                "on" | "true" | "yes" => true,
                "off" | "false" | "no" => false,
                _ => {
                    push_error(app, &format!("expected on/off, got {value:?}"));
                    return;
                }
            };
            match session.set_config_option(&id, SessionConfigOptionValue::boolean(parsed)) {
                Ok(()) => {
                    if let SessionConfigKind::Boolean(b) = &mut app.config[ix].kind {
                        b.current_value = parsed;
                    }
                    let state = if parsed { "on" } else { "off" };
                    push_notice(app, &format!("{option_name} → {state}"));
                }
                Err(e) => push_error(
                    app,
                    &format!("set config failed: {}", session::error_text(&e)),
                ),
            }
        }
        _ => push_error(app, &format!("{option_name}: unsupported option kind")),
    }
}

/// Bracketed paste: newline-normalize, collapse large pastes to a
/// placeholder (the real text is stashed and expanded on submit).
fn handle_paste(text: &str, app: &mut App) {
    let text = text.replace("\r\n", "\n").replace('\r', "\n");
    let lines = text.lines().count();
    if lines >= LARGE_PASTE_LINES || text.chars().count() > LARGE_PASTE_CHARS {
        app.paste_seq += 1;
        let placeholder = format!("[pasted #{} — {} lines]", app.paste_seq, lines.max(1));
        app.input.insert_text(&placeholder);
        app.pending_pastes.push((placeholder, text));
    } else {
        app.input.insert_text(&text);
    }
}

/// Swap paste placeholders back for their real text; stale stashes
/// (placeholder edited away) are dropped.
fn expand_pastes(app: &mut App, content: &str) -> String {
    let mut out = content.to_owned();
    for (placeholder, text) in app.pending_pastes.drain(..) {
        if out.contains(&placeholder) {
            out = out.replace(&placeholder, &text);
        }
    }
    out
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
        app.buffer
            .push(ChatEntry::Text(vec![Line::from(Span::styled(
                format!("  [queued] {content}"),
                Style::new().add_modifier(Modifier::DIM),
            ))]));
    } else {
        send_prompt(app, session, &content);
    }
}

fn send_prompt(app: &mut App, session: &Session, content: &str) {
    let (blocks, attached) = mention_blocks(content, &session.cwd, app.embed_context);
    match session.prompt_blocks(blocks) {
        Ok(()) => {
            app.streaming = true;
            app.buffer.start_waiting();
            for path in attached {
                push_notice(app, &format!("attached {path}"));
            }
        }
        Err(e) => push_error(app, &format!("prompt failed: {e}")),
    }
}

/// Cap on a single `@`-mentioned file embedded into a prompt.
const MENTION_MAX_BYTES: u64 = 128 * 1024;

/// The prompt's content blocks: the text first, then one embedded
/// resource per readable `@path` mention (deduped). Returns the block
/// list and the paths that were attached.
fn mention_blocks(
    content: &str,
    cwd: &std::path::Path,
    enabled: bool,
) -> (Vec<ContentBlock>, Vec<String>) {
    let mut blocks: Vec<ContentBlock> = vec![content.to_owned().into()];
    let mut attached = Vec::new();
    if !enabled {
        return (blocks, attached);
    }
    for token in content.split_whitespace() {
        let Some(path_str) = token.strip_prefix('@') else {
            continue;
        };
        let path_str = path_str.trim_end_matches([',', '.', ';', ':', ')', '?', '!', '"', '\'']);
        if path_str.is_empty() || attached.iter().any(|p| p == path_str) {
            continue;
        }
        let path = cwd.join(path_str);
        let small =
            std::fs::metadata(&path).is_ok_and(|m| m.is_file() && m.len() <= MENTION_MAX_BYTES);
        if !small {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        let uri = format!("file://{}", path.display());
        blocks.push(ContentBlock::Resource(EmbeddedResource::new(
            EmbeddedResourceResource::TextResourceContents(TextResourceContents::new(text, uri)),
        )));
        attached.push(path_str.to_owned());
    }
    (blocks, attached)
}

/// Workspace file list for `@` completion — git-tracked files, or
/// nothing outside a repo (no walker needed for the common case).
fn workspace_files(cwd: &std::path::Path) -> Vec<String> {
    let output = std::process::Command::new("git")
        .arg("ls-files")
        .current_dir(cwd)
        .output();
    match output {
        Ok(out) if out.status.success() => String::from_utf8_lossy(&out.stdout)
            .lines()
            .map(str::to_owned)
            .collect(),
        _ => Vec::new(),
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
    app.buffer.finish();
    app.streaming = false;
    match reason {
        StopReason::EndTurn => {}
        StopReason::Cancelled => {
            app.buffer.fail_running_tools();
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
    app.buffer.finish();
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
            app.buffer.push_text(&content_text(&chunk.content));
        }
        SessionUpdate::AgentThoughtChunk(chunk) => {
            app.buffer.push_thinking(&content_text(&chunk.content));
        }
        SessionUpdate::ToolCall(call) => {
            let id = call.tool_call_id.to_string();
            app.buffer
                .push_tool_call(&id, call.title, kind_glyph(call.kind));
            if !call.locations.is_empty() {
                app.buffer
                    .set_tool_locations(&id, format_locations(&call.locations));
            }
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
                app.buffer.set_tool_label(&id, title);
            }
            if let Some(kind) = update.fields.kind {
                app.buffer.set_tool_glyph(&id, kind_glyph(kind));
            }
            if let Some(locations) = update.fields.locations {
                app.buffer
                    .set_tool_locations(&id, format_locations(&locations));
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
        SessionUpdate::CurrentModeUpdate(update) => {
            app.current_mode = Some(update.current_mode_id.to_string());
        }
        SessionUpdate::ConfigOptionUpdate(update) => {
            app.config = update.config_options;
        }
        SessionUpdate::UsageUpdate(update) => {
            app.usage = Some(Usage {
                used: update.used,
                size: update.size,
                cost: update.cost.map(|c| (c.amount, c.currency)),
            });
        }
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
            app.buffer.set_plan(rows);
        }
        // Live turns are echoed locally at submit; chunks only arrive
        // here as replayed history from `session/load`.
        SessionUpdate::UserMessageChunk(chunk) => {
            app.buffer.push_user_chunk(&content_text(&chunk.content));
        }
        _ => {}
    }
}

/// ASCII hints for the tool kind, shown after the status marker.
fn kind_glyph(kind: ToolKind) -> &'static str {
    match kind {
        ToolKind::Read => "<",
        ToolKind::Edit => ">",
        ToolKind::Delete => "x",
        ToolKind::Move => "=",
        ToolKind::Search => "*",
        ToolKind::Execute => "$",
        ToolKind::Think => "~",
        ToolKind::Fetch => "%",
        ToolKind::SwitchMode => "@",
        _ => "",
    }
}

/// `path:line` strings, with the cwd prefix stripped for brevity.
fn format_locations(locations: &[ToolCallLocation]) -> Vec<String> {
    let cwd = std::env::current_dir().unwrap_or_default();
    locations
        .iter()
        .map(|location| {
            let path = location
                .path
                .strip_prefix(&cwd)
                .unwrap_or(&location.path)
                .display();
            match location.line {
                Some(line) => format!("{path}:{line}"),
                None => path.to_string(),
            }
        })
        .collect()
}

fn apply_tool_status(app: &mut App, id: &str, status: ToolCallStatus) {
    let mapped = match status {
        ToolCallStatus::Completed => ToolStatus::Success,
        ToolCallStatus::Failed => ToolStatus::Failure,
        _ => ToolStatus::Running,
    };
    app.buffer.set_tool_status(id, mapped);
}

fn push_tool_content(app: &mut App, id: &str, content: &[ToolCallContent], failed: bool) {
    let mut text: Vec<String> = Vec::new();
    for item in content {
        match item {
            ToolCallContent::Content(inner) => text.push(content_text(&inner.content)),
            ToolCallContent::Diff(diff) => {
                if !text.is_empty() {
                    app.buffer
                        .push_tool_result(id, &std::mem::take(&mut text).join("\n"), failed);
                }
                app.buffer.push_tool_diff(
                    id,
                    &diff.path.display().to_string(),
                    diff.old_text.as_deref().unwrap_or(""),
                    &diff.new_text,
                );
            }
            ToolCallContent::Terminal(_) => text.push("[terminal]".to_owned()),
            _ => text.push("[content]".to_owned()),
        }
    }
    let text = text.join("\n");
    if !text.is_empty() {
        app.buffer.push_tool_result(id, &text, failed);
    }
}

fn content_text(block: &ContentBlock) -> String {
    match block {
        ContentBlock::Text(text) => text.text.clone(),
        ContentBlock::Resource(resource) => match &resource.resource {
            EmbeddedResourceResource::TextResourceContents(text) => {
                format!("[resource: {}]", text.uri)
            }
            EmbeddedResourceResource::BlobResourceContents(blob) => {
                format!("[resource: {}]", blob.uri)
            }
            _ => "[resource]".to_owned(),
        },
        ContentBlock::ResourceLink(link) => format!("[{}]({})", link.name, link.uri),
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
    app.buffer
        .push(ChatEntry::Text(vec![Line::from(Span::styled(
            format!("  {message}"),
            Style::new().fg(Color::Indexed(204)),
        ))]));
}

fn push_notice(app: &mut App, message: &str) {
    app.buffer
        .push(ChatEntry::Text(vec![Line::from(Span::styled(
            format!("  [{message}]"),
            Style::new().add_modifier(Modifier::DIM),
        ))]));
}

fn push_help(app: &mut App) {
    let lines = [
        "  /clear — clear the transcript",
        "  /mode  — list or switch session modes",
        "  /config — list or set config options (model etc.)",
        "  /mcp   — add and toggle MCP servers",
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
    app.buffer.push(ChatEntry::Text(lines));
}

// ── Drawing ──────────────────────────────────────────────────────

/// Set the terminal title via OSC, deduped and sanitized (control and
/// bidi-formatting codepoints stripped so agent-controlled titles
/// can't smuggle escapes).
fn apply_title(app: &mut App) {
    let raw = if app.chat_title.is_empty() {
        format!("cydonia — {}", app.agent)
    } else {
        format!("cydonia — {}", app.chat_title)
    };
    let title: String = raw
        .chars()
        .filter(|c| {
            !c.is_control() && !matches!(c, '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}')
        })
        .take(MAX_TITLE_CHARS)
        .collect();
    if title != app.last_title {
        let _ = execute!(std::io::stdout(), SetTitle(&title));
        app.last_title = title;
    }
}

fn draw(frame: &mut ratatui::Frame, app: &mut App) {
    let input_height = app.input.height().min(frame.area().height / 3).max(3);
    let hint = app.quit_hint.is_some();
    let mut constraints = vec![Constraint::Min(1)];
    if hint {
        constraints.push(Constraint::Length(1));
    }
    constraints.push(Constraint::Length(input_height));
    let chunks = Layout::vertical(constraints).split(frame.area());

    draw_chat(frame, chunks[0], app);
    if hint {
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                "  press Ctrl+C again to quit",
                Style::new().add_modifier(Modifier::DIM),
            ))),
            chunks[1],
        );
    }
    app.input.render(
        frame,
        chunks[chunks.len() - 1],
        &app.agent,
        &app.chat_title,
        &status_line(app),
    );

    if let Some(ref prompt) = app.permission {
        draw_permission(frame, prompt);
    }
    if let Some(ref picker) = app.mcp {
        picker.draw(frame);
    }
}

/// Mode · context% · cost, shown on the input's bottom border.
fn status_line(app: &App) -> String {
    let mut parts = Vec::new();
    if let Some(id) = &app.current_mode {
        let name = app
            .modes
            .iter()
            .find(|(mid, _)| mid == id)
            .map(|(_, name)| name.as_str())
            .unwrap_or(id);
        parts.push(name.to_owned());
    }
    if let Some(usage) = &app.usage {
        if let Some(pct) = usage.used.saturating_mul(100).checked_div(usage.size) {
            parts.push(format!("{pct}%"));
        }
        if let Some((amount, currency)) = &usage.cost {
            if currency == "USD" {
                parts.push(format!("${amount:.2}"));
            } else {
                parts.push(format!("{amount:.2} {currency}"));
            }
        }
    }
    parts.join(" · ")
}

fn draw_chat(frame: &mut ratatui::Frame, area: Rect, app: &mut App) {
    let width = (area.width as usize).saturating_sub(2);
    let mut lines = app.buffer.lines(app.frame_count, width);
    if app.buffer.waiting {
        let spinner = if app.frame_count % 12 < 6 { "⏺" } else { " " };
        lines.push(Line::from(Span::styled(
            spinner,
            Style::new().add_modifier(Modifier::DIM),
        )));
    }

    let total_lines = lines.len() as u16;
    let max_scroll = total_lines.saturating_sub(area.height);
    let top = if app.scroll == usize::MAX {
        max_scroll
    } else {
        (app.scroll.min(u16::MAX as usize) as u16).min(max_scroll)
    };
    app.last_scroll_top = top;
    app.last_max_scroll = max_scroll;

    frame.render_widget(Paragraph::new(lines).scroll((top, 0)), area);
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
