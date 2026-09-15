//! One ACP session over a spawned agent subprocess.
//!
//! Two things here are load-bearing:
//! - All agent-side events flow through ONE channel. cacp's read loop awaits
//!   each notification before it reads the next frame, so a single channel
//!   preserves the exact wire order (a turn's final updates arrive before its
//!   result — separate channels lose that).
//! - The connection runs on its own tokio runtime. cacp spawns its read and
//!   write loops with `tokio::spawn`, and gpui's executor is smol's.

use crate::{
    agent::{mcp, serve},
    model::settings,
};
use anyhow::{Result, anyhow};
use cacp::{
    AgentConn, Client, Direction, Error, Tap,
    schema::{
        AuthenticateRequest, CancelNotification, ClientCapabilities, ContentBlock, EnvVariable,
        FileSystemCapabilities, HttpHeader, InitializeRequest, InitializeResponse,
        LoadSessionRequest, McpServer, McpServerHttp, McpServerStdio, NewSessionRequest,
        NewSessionResponse, PromptRequest, ReadTextFileRequest, ReadTextFileResponse,
        RequestPermissionRequest, RequestPermissionResponse, SessionConfigOptionValue, SessionId,
        SessionNotification, SessionUpdate, SetSessionConfigOptionRequest, SetSessionModeRequest,
        StopReason, WriteTextFileRequest, WriteTextFileResponse,
    },
};
use std::process::Stdio;
use std::{
    path::PathBuf,
    sync::{Arc, OnceLock},
    time::Duration,
};
use tokio::{
    io::{AsyncBufReadExt, BufReader},
    process::{Child, ChildStderr, Command},
    runtime::Runtime,
    sync::{mpsc, oneshot},
};

/// Where the protocol tap writes, and the switch that echoes an agent's stderr
/// to ours on the way past — it reaches the transcript either way.
const DEBUG: &str = "CYDONIA_DEBUG";

/// What our own tool server is called to an agent. Also the namespace every
/// tool of ours comes back under, so the two are read from one place.
const SERVER: &str = "cydonia";

/// How long a `session/cancel` already on the wire is given to land before the
/// process is killed under it.
///
/// Short, because it is all this can buy. Closing the agent's stdin — the thing
/// that would actually let it wind itself up — is not reachable from here:
/// cacp's read loop holds a `Peer` of its own, so the write loop that owns
/// stdin outlives every handle this side can drop.
const SHUTDOWN_GRACE: Duration = Duration::from_millis(250);

/// The runtime every connection runs on, started on first use.
pub fn runtime() -> &'static Runtime {
    static RUNTIME: OnceLock<Runtime> = OnceLock::new();
    RUNTIME.get_or_init(|| Runtime::new().expect("failed to start the tokio runtime"))
}

/// Everything the agent side feeds into the frontend, in wire order.
pub enum Event {
    Update(SessionUpdate),
    /// The agent asks the user to authorize a tool call.
    Permission(RequestPermissionRequest, Reply<RequestPermissionResponse>),
    /// One line the agent wrote to its stderr. Not protocol — the runtime
    /// under it talking, or the shell that could not start it — so it arrives
    /// off its own task and only approximately in step with the rest.
    Stderr(String),
    /// The prompt turn settled: its stop reason, or the agent's error.
    TurnDone(Result<StopReason, Error>),
    /// The agent's read loop ended — the process died or closed its stdout.
    /// Last in wire order, so any final updates land before the frontend
    /// gives the session up.
    Closed,
}

/// The frontend's receiving end.
pub type Events = mpsc::UnboundedReceiver<Event>;

/// The agent side's sending end.
pub type Sender = mpsc::UnboundedSender<Event>;

/// Open the channel a session runs on.
///
/// The caller holds both halves and lends the sender to [`Session::spawn`],
/// rather than being given the pair back by it. A launch that fails is why:
/// the process can write to its stderr and die without ever answering
/// `initialize`, and a receiver created inside the launch would be dropped
/// with the error, taking the only account of what went wrong with it.
pub fn channel() -> (Sender, Events) {
    mpsc::unbounded_channel()
}

/// The answer half of a request the frontend has to make. Dropping it
/// declines the request rather than hanging the agent.
pub struct Reply<T>(oneshot::Sender<Result<T, Error>>);

impl<T> Reply<T> {
    pub fn send(self, value: T) {
        let _ = self.0.send(Ok(value));
    }
}

/// A live session: the connection, its identity, and the agent process.
pub struct Session {
    /// `Some` for the whole of a session's life — emptied only by
    /// [`Session::drop`], which has to take it to close it before the process.
    conn: Option<AgentConn>,
    tx: mpsc::UnboundedSender<Event>,
    /// The agent process. Held in an `Option` for the same reason as `conn`.
    child: Option<Child>,
    pub session_id: SessionId,
    pub init: InitializeResponse,
    pub response: NewSessionResponse,
    pub cwd: PathBuf,
    /// True when an existing session was loaded (history replayed as
    /// queued [`Event::Update`]s) instead of a fresh one created.
    pub loaded: bool,
    built_in_mcp: bool,
}

/// How to open a session.
#[derive(Default)]
pub struct Launch {
    /// The session's working directory.
    pub cwd: PathBuf,
    /// A session to load instead of starting fresh.
    pub previous: Option<String>,
}

impl Launch {
    pub fn new(cwd: PathBuf) -> Self {
        Self {
            cwd,
            ..Default::default()
        }
    }
}

impl Session {
    /// Spawn `entry` over stdio, initialize, and open a session. The agent
    /// dies with the returned [`Session`].
    ///
    /// With [`Launch::previous`] set and the agent capable, `session/load`
    /// replays that session's history instead of starting fresh; a failed
    /// load (stale id, agent restart) falls back to a new session.
    pub async fn spawn(entry: &settings::Agent, launch: Launch, tx: Sender) -> Result<Self> {
        let mut command = Command::new(&entry.command);
        command.args(&entry.args).envs(&entry.env);
        // HTTP clients can inherit a system proxy that does not exempt IP
        // loopback addresses. Our MCP server must be reached directly. Merge
        // both spellings because agents differ in which one they honor.
        let bypass = loopback_bypass(["NO_PROXY", "no_proxy"].map(|key| {
            entry
                .env
                .get(key)
                .cloned()
                .or_else(|| std::env::var(key).ok())
        }));
        command.env("NO_PROXY", &bypass).env("no_proxy", &bypass);
        // An agent's diagnostics are not this app's to print. cacp leaves the
        // choice to the caller — "a TUI usually wants it captured and a CLI
        // usually does not" — and a desktop app that inherits them sprays a
        // node SDK's teardown chatter over whichever terminal happened to
        // launch it, about a shutdown the user asked for.
        //
        // Captured rather than discarded, though, because it is the only
        // account of a process that dies without ever speaking protocol: the
        // `#!/usr/bin/env node` shim that found no node, the package that
        // would not resolve. It reaches the transcript as the execution it is
        // — see [`Event::Stderr`]. `CYDONIA_DEBUG`, which already redirects
        // the protocol tap, still echoes the lines where a developer looks.
        command.stderr(Stdio::piped());

        let configured = mcp::servers();

        let (conn, mut child) =
            cacp::spawn(&mut command, Arc::new(Frontend(tx.clone())), debug_tap())
                .map_err(|e| anyhow!("failed to start {}: {}", entry.command, error_text(&e)))?;
        if let Some(stderr) = child.stderr.take() {
            runtime().spawn(drain(stderr, tx.clone()));
        }

        Self::open(conn, child, tx, launch, configured).await
    }

    async fn open(
        conn: AgentConn,
        child: Child,
        tx: mpsc::UnboundedSender<Event>,
        launch: Launch,
        configured: Vec<mcp::McpServer>,
    ) -> Result<Self> {
        let cwd = launch.cwd.clone();
        let init = conn
            .initialize(InitializeRequest::new(ClientCapabilities {
                fs: FileSystemCapabilities {
                    read_text_file: true,
                    write_text_file: true,
                    meta: None,
                },
                ..Default::default()
            }))
            .await
            .map_err(|e| anyhow!("initialize failed: {}", error_text(&e)))?;

        // Only now are the agent's MCP capabilities known, so remote
        // servers can be dropped for agents that can't reach them.
        let (mcp_servers, built_in_mcp) = acp_mcp_servers(&configured, &init, &cwd);

        let mut loaded = false;
        let mut response = None;
        if let Some(id) = launch
            .previous
            .clone()
            .filter(|_| init.agent_capabilities.load_session)
        {
            let load = || LoadSessionRequest {
                mcp_servers: mcp_servers.clone(),
                ..LoadSessionRequest::new(id.clone(), cwd.clone())
            };
            let result = match conn.load_session(load()).await {
                Err(e) if e.is_auth_required() => {
                    authenticate(&conn, &init).await?;
                    conn.load_session(load()).await
                }
                other => other,
            };
            // A failed load (stale id, agent state gone) falls through
            // to a fresh session rather than failing the launch.
            if let Ok(load_response) = result {
                response = Some(NewSessionResponse {
                    session_id: id.clone().into(),
                    modes: load_response.modes,
                    config_options: load_response.config_options,
                    meta: None,
                });
                loaded = true;
            }
        }

        let response = match response {
            Some(response) => response,
            None => {
                let new_session = || NewSessionRequest {
                    mcp_servers: mcp_servers.clone(),
                    ..NewSessionRequest::new(cwd.clone())
                };
                match conn.new_session(new_session()).await {
                    Ok(response) => response,
                    Err(e) if e.is_auth_required() => {
                        authenticate(&conn, &init).await?;
                        conn.new_session(new_session()).await.map_err(|e| {
                            anyhow!(
                                "session/new failed after authentication: {}",
                                error_text(&e)
                            )
                        })?
                    }
                    Err(e) => return Err(anyhow!("session/new failed: {}", error_text(&e))),
                }
            }
        };

        Ok(Self {
            conn: Some(conn),
            tx,
            child: Some(child),
            session_id: response.session_id.clone(),
            init,
            response,
            cwd,
            loaded,
            built_in_mcp,
        })
    }

    /// A handle on the agent. Cheap to clone, and present for as long as
    /// anything can reach the session — the `Option` is [`Session::drop`]'s.
    fn conn(&self) -> AgentConn {
        self.conn.clone().expect("the session is being dropped")
    }

    /// Send a prompt turn. Its result arrives as [`Event::TurnDone`] —
    /// including a failure to send it at all.
    pub fn prompt(&self, content: &str) {
        self.prompt_blocks(vec![content.to_owned().into()]);
    }

    /// Send a prompt turn with explicit content blocks (text plus
    /// embedded resources). Same result path as [`Self::prompt`].
    pub fn prompt_blocks(&self, blocks: Vec<ContentBlock>) {
        let blocks = super::context::prompt(&self.cwd, self.built_in_mcp, blocks);
        let request = PromptRequest::new(self.session_id.clone(), blocks);
        let conn = self.conn();
        let tx = self.tx.clone();
        runtime().spawn(async move {
            let done = conn
                .prompt(request)
                .await
                .map(|response| response.stop_reason);
            let _ = tx.send(Event::TurnDone(done));
        });
    }

    /// Cancel the in-flight turn (`session/cancel`). The turn still ends
    /// with an [`Event::TurnDone`] carrying `StopReason::Cancelled`. Pending
    /// permission replies are the frontend's to answer `Cancelled`.
    pub fn cancel(&self) -> Result<(), Error> {
        self.conn().cancel(CancelNotification {
            session_id: self.session_id.clone(),
            meta: None,
        })
    }

    /// Switch the session mode (`session/set_mode`). Fire-and-forget:
    /// frontends validate the id against `response.modes` up front, and
    /// the agent's `CurrentModeUpdate` is the confirmation.
    pub fn set_mode(&self, mode_id: &str) {
        let request = SetSessionModeRequest {
            session_id: self.session_id.clone(),
            mode_id: mode_id.into(),
            meta: None,
        };
        let conn = self.conn();
        runtime().spawn(async move { conn.set_session_mode(request).await });
    }

    /// Set a session config option (`session/set_config_option`).
    /// Fire-and-forget like [`Self::set_mode`]: frontends validate
    /// against `response.config_options`, and the agent's
    /// `ConfigOptionUpdate` is the confirmation.
    pub fn set_config_option(&self, config_id: &str, value: SessionConfigOptionValue) {
        let request = SetSessionConfigOptionRequest {
            session_id: self.session_id.clone(),
            config_id: config_id.into(),
            value,
            meta: None,
        };
        let conn = self.conn();
        runtime().spawn(async move { conn.set_session_config_option(request).await });
    }
}

/// Serves what the agent asks of us: file access answered here, anything
/// the user has to see queued for the frontend.
struct Frontend(mpsc::UnboundedSender<Event>);

impl Client for Frontend {
    async fn session_update(&self, notification: SessionNotification) {
        let _ = self.0.send(Event::Update(notification.update));
    }

    async fn request_permission(
        &self,
        request: RequestPermissionRequest,
    ) -> Result<RequestPermissionResponse, Error> {
        let (tx, rx) = oneshot::channel();
        self.0
            .send(Event::Permission(request, Reply(tx)))
            .map_err(|_| Error::internal_error().data("the frontend is gone"))?;
        rx.await.unwrap_or_else(|_| Err(Error::method_not_found()))
    }

    async fn read_text_file(
        &self,
        request: ReadTextFileRequest,
    ) -> Result<ReadTextFileResponse, Error> {
        read_text_file(&request)
    }

    async fn write_text_file(
        &self,
        request: WriteTextFileRequest,
    ) -> Result<WriteTextFileResponse, Error> {
        std::fs::write(&request.path, &request.content)
            .map(|()| WriteTextFileResponse::default())
            .map_err(|e| io_error(&request.path, &e))
    }
}

/// Take the agent down: the connection first, then the process behind it.
///
/// `cacp::spawn` sets `kill_on_drop`, so letting the child field drop on its own
/// is an immediate SIGKILL — mid-request, if the agent was answering one.
/// [`crate::model::session::ChatSession::close`] sends `session/cancel` ahead
/// of this, and the pause here is what gives that notification time to be read.
///
/// It is not a clean shutdown, and cannot be until cacp can close an agent's
/// stdin: its read loop is handed a `Peer` by value, so the write loop holding
/// stdin lives as long as the agent does, whatever this side drops. Until then
/// the kill is the only exit and the agent's stderr is where the noise goes —
/// see [`DEBUG`].
impl Drop for Session {
    fn drop(&mut self) {
        let (Some(conn), Some(mut child)) = (self.conn.take(), self.child.take()) else {
            return;
        };
        drop(conn);
        runtime().spawn(async move {
            let _ = tokio::time::timeout(SHUTDOWN_GRACE, child.wait()).await;
        });
    }
}

/// cacp drops the client when its read loop ends, which is the only notice
/// the frontend gets that the agent is gone.
impl Drop for Frontend {
    fn drop(&mut self) {
        let _ = self.0.send(Event::Closed);
    }
}

// A GPUI (or any multi-threaded) frontend holds `Session` in its UI state
// and moves `Event` — reply included — across executor threads.
const _: () = {
    const fn assert_send<T: Send>() {}
    assert_send::<Session>();
    assert_send::<Event>();
};

fn loopback_bypass(existing: [Option<String>; 2]) -> String {
    let mut entries = Vec::new();
    for value in existing
        .iter()
        .flatten()
        .map(String::as_str)
        .chain(["localhost,127.0.0.1,::1"])
    {
        for entry in value
            .split(',')
            .map(str::trim)
            .filter(|entry| !entry.is_empty())
        {
            if !entries.contains(&entry) {
                entries.push(entry);
            }
        }
    }
    entries.join(",")
}

/// The enabled servers an agent can actually reach, in ACP's shape.
/// Remote servers are dropped for agents that don't advertise HTTP MCP
/// rather than being sent and failing.
///
/// Cydonia's own door goes first, when the agent can reach it. It is not in
/// `mcp.toml` and must not be — that file is the servers the user added, and
/// this one is not the user's to remove.
fn acp_mcp_servers(
    configured: &[mcp::McpServer],
    init: &InitializeResponse,
    cwd: &std::path::Path,
) -> (Vec<McpServer>, bool) {
    let http = init.agent_capabilities.mcp_capabilities.http;
    let ours = http.then(serve::url).flatten().map(|url| {
        McpServer::Http(McpServerHttp {
            name: SERVER.to_owned(),
            url,
            // Which project this session is. The tools then take no directory
            // at all — one a session's model had to supply is one it could
            // supply wrongly, about something already known here.
            headers: vec![{
                let (name, value) = serve::project(cwd);
                HttpHeader {
                    name: name.to_owned(),
                    value,
                    meta: None,
                }
            }],
            meta: None,
        })
    });
    let available = ours.is_some();
    let servers = ours
        .into_iter()
        .chain(
            configured
                .iter()
                .filter(|server| server.enabled)
                .filter_map(|server| match (&server.command, &server.url) {
                    (Some(command), _) => Some(McpServer::Stdio(McpServerStdio {
                        name: server.name.clone(),
                        command: command.into(),
                        args: server.args.clone(),
                        env: server
                            .env
                            .iter()
                            .map(|(name, value)| EnvVariable {
                                name: name.clone(),
                                value: value.clone(),
                                meta: None,
                            })
                            .collect(),
                        meta: None,
                    })),
                    (None, Some(url)) if http => Some(McpServer::Http(McpServerHttp {
                        name: server.name.clone(),
                        url: url.clone(),
                        headers: Vec::new(),
                        meta: None,
                    })),
                    _ => None,
                }),
        )
        .collect();
    (servers, available)
}

/// Try each advertised auth method in order. Non-interactive methods
/// (API keys read from the agent's env) fail fast when unset;
/// interactive ones (OAuth) block until the user completes the flow in
/// the browser the agent opens.
async fn authenticate(conn: &AgentConn, init: &InitializeResponse) -> Result<()> {
    if init.auth_methods.is_empty() {
        return Err(anyhow!(
            "authentication required, but the agent advertises no auth methods"
        ));
    }
    let mut failures = Vec::new();
    for method in &init.auth_methods {
        let request = AuthenticateRequest {
            method_id: method.id().clone(),
            meta: None,
        };
        match conn.authenticate(request).await {
            Ok(_) => return Ok(()),
            Err(e) => failures.push(format!("{}: {}", method.name(), error_text(&e))),
        }
    }
    Err(anyhow!("authentication failed — {}", failures.join("; ")))
}

/// Read the agent's stderr to its end, a line at a time, onto the channel the
/// rest of the session runs on.
///
/// Ends when the pipe closes, which a dead process is what does — so this
/// task is also what lets the channel close behind a launch that failed, and
/// the caller stop waiting on it.
async fn drain(stderr: ChildStderr, tx: Sender) {
    let echo = std::env::var_os(DEBUG).is_some();
    let mut lines = BufReader::new(stderr).lines();
    while let Ok(Some(line)) = lines.next_line().await {
        if echo {
            eprintln!("{line}");
        }
        if tx.send(Event::Stderr(line)).is_err() {
            return;
        }
    }
}

/// Spend a loaded session's replay, keeping only what is state rather than
/// transcript.
///
/// `session/load` replays the whole conversation before it answers, and the
/// client is holding that transcript already — the replay would arrive as a
/// second copy of what is on screen. Only updates can be queued at this point,
/// because nothing else is sent until we prompt.
///
/// What survives is the one thing in a replay that is not transcript: what the
/// conversation has already spent. Nothing in ACP asks for that — it arrives
/// as a notification or not at all — so dropping it with the rest is what
/// leaves a resumed session reading empty until its next turn. The last one
/// wins, and goes back on the channel the frontend is about to read.
///
/// Stderr is left where it is. It is not part of any replay: it is this
/// launch's own process talking, and it has as much right to the transcript
/// here as anywhere.
pub fn spend_replay(events: &mut Events, tx: &Sender) {
    let mut usage = None;
    let mut kept = Vec::new();
    while let Ok(event) = events.try_recv() {
        match event {
            Event::Update(SessionUpdate::UsageUpdate(update)) => usage = Some(update),
            Event::Update(_) => {}
            other => kept.push(other),
        }
    }
    if let Some(update) = usage {
        let _ = tx.send(Event::Update(SessionUpdate::UsageUpdate(update)));
    }
    for event in kept {
        let _ = tx.send(event);
    }
}

/// One-line rendering of a JSON-RPC error (`Display` dumps a JSON blob).
pub fn error_text(e: &Error) -> String {
    match &e.data {
        Some(data) => {
            let detail = data
                .as_str()
                .map(str::to_owned)
                .unwrap_or_else(|| data.to_string());
            format!("{} — {detail}", e.message)
        }
        None => e.message.clone(),
    }
}

/// Serve `fs/read_text_file`: whole file, or 1-based `line` + `limit` window.
fn read_text_file(request: &ReadTextFileRequest) -> Result<ReadTextFileResponse, Error> {
    let content =
        std::fs::read_to_string(&request.path).map_err(|e| io_error(&request.path, &e))?;
    let content = match (request.line, request.limit) {
        (None, None) => content,
        (line, limit) => {
            let skip = line.map(|l| l.saturating_sub(1) as usize).unwrap_or(0);
            let take = limit.map(|l| l as usize).unwrap_or(usize::MAX);
            content
                .lines()
                .skip(skip)
                .take(take)
                .collect::<Vec<_>>()
                .join("\n")
        }
    };
    Ok(ReadTextFileResponse {
        content,
        meta: None,
    })
}

fn io_error(path: &std::path::Path, e: &std::io::Error) -> Error {
    Error::internal_error().data(format!("{}: {e}", path.display()))
}

/// With `CYDONIA_DEBUG=<path>` set, append every JSON-RPC line to that file.
fn debug_tap() -> Option<Tap> {
    let path = std::env::var(DEBUG).ok()?;
    Some(Arc::new(move |direction: Direction, line: &str| {
        use std::io::Write;
        if let Ok(mut f) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
        {
            let _ = writeln!(f, "{direction:?}: {line}");
        }
    }))
}

#[cfg(test)]
#[path = "../../tests/unit/acp_proxy.rs"]
mod proxy_tests;
