//! One ACP session over a spawned agent subprocess.
//!
//! Two things here are load-bearing:
//! - All agent-side events flow through ONE channel. cacp's read loop awaits
//!   each notification before it reads the next frame, so a single channel
//!   preserves the exact wire order (a turn's final updates arrive before its
//!   result — separate channels lose that).
//! - The connection runs on its own tokio runtime. cacp spawns its read and
//!   write loops with `tokio::spawn`, and gpui's executor is smol's.

use crate::{agent::mcp, model::settings};
use anyhow::{Result, anyhow};
use cacp::{
    AgentConn, Client, Direction, Error, Tap,
    schema::{
        AuthenticateRequest, CancelNotification, ClientCapabilities, ContentBlock, EnvVariable,
        FileSystemCapabilities, InitializeRequest, InitializeResponse, LoadSessionRequest,
        McpServer, McpServerHttp, McpServerStdio, NewSessionRequest, NewSessionResponse,
        PromptRequest, ReadTextFileRequest, ReadTextFileResponse, RequestPermissionRequest,
        RequestPermissionResponse, SessionConfigOptionValue, SessionId, SessionNotification,
        SessionUpdate, SetSessionConfigOptionRequest, SetSessionModeRequest, StopReason,
        WriteTextFileRequest, WriteTextFileResponse,
    },
};
use std::{
    path::PathBuf,
    sync::{Arc, OnceLock},
};
use tokio::{
    process::{Child, Command},
    runtime::Runtime,
    sync::{mpsc, oneshot},
};

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
    /// The prompt turn settled: its stop reason, or the agent's error.
    TurnDone(Result<StopReason, Error>),
    /// The agent's read loop ended — the process died or closed its stdout.
    /// Last in wire order, so any final updates land before the frontend
    /// gives the session up.
    Closed,
}

/// The frontend's receiving end, handed back beside the [`Session`] so an
/// event loop can poll it while the session handle stays borrowable for
/// `prompt`/`cancel`.
pub type Events = mpsc::UnboundedReceiver<Event>;

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
    conn: AgentConn,
    tx: mpsc::UnboundedSender<Event>,
    /// Dropping this kills the agent.
    _child: Child,
    pub session_id: SessionId,
    pub init: InitializeResponse,
    pub response: NewSessionResponse,
    pub cwd: PathBuf,
    /// True when an existing session was loaded (history replayed as
    /// queued [`Event::Update`]s) instead of a fresh one created.
    pub loaded: bool,
}

/// Progress reporter for the steps before the frontend is up.
pub type StatusFn = Box<dyn Fn(&str) + Send + Sync>;

/// How to open a session.
#[derive(Default)]
pub struct Launch {
    /// The session's working directory.
    pub cwd: PathBuf,
    /// A session to load instead of starting fresh.
    pub previous: Option<String>,
    /// Progress for the steps before the frontend is up — notably
    /// authentication, which can block on a browser sign-in.
    pub status: Option<StatusFn>,
}

impl Launch {
    pub fn new(cwd: PathBuf) -> Self {
        Self {
            cwd,
            ..Default::default()
        }
    }

    fn say(&self, message: &str) {
        if let Some(status) = &self.status {
            status(message);
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
    pub async fn spawn(entry: &settings::Agent, launch: Launch) -> Result<(Self, Events)> {
        let mut command = Command::new(&entry.command);
        command.args(&entry.args).envs(&entry.env);

        let configured = mcp::servers();

        let (tx, events) = mpsc::unbounded_channel();
        let (conn, child) = cacp::spawn(&mut command, Arc::new(Frontend(tx.clone())), debug_tap())
            .map_err(|e| anyhow!("failed to start {}: {}", entry.command, error_text(&e)))?;

        let session = Self::open(conn, child, tx, launch, configured).await?;
        Ok((session, events))
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
        let mcp_servers = acp_mcp_servers(&configured, &init);

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
                    authenticate(&conn, &init, &launch).await?;
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
                        authenticate(&conn, &init, &launch).await?;
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
            conn,
            tx,
            _child: child,
            session_id: response.session_id.clone(),
            init,
            response,
            cwd,
            loaded,
        })
    }

    /// Send a prompt turn. Its result arrives as [`Event::TurnDone`] —
    /// including a failure to send it at all.
    pub fn prompt(&self, content: &str) {
        self.prompt_blocks(vec![content.to_owned().into()]);
    }

    /// Send a prompt turn with explicit content blocks (text plus
    /// embedded resources). Same result path as [`Self::prompt`].
    pub fn prompt_blocks(&self, blocks: Vec<ContentBlock>) {
        let request = PromptRequest::new(self.session_id.clone(), blocks);
        let conn = self.conn.clone();
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
        self.conn.cancel(CancelNotification {
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
        let conn = self.conn.clone();
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
        let conn = self.conn.clone();
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

/// The enabled servers an agent can actually reach, in ACP's shape.
/// Remote servers are dropped for agents that don't advertise HTTP MCP
/// rather than being sent and failing.
fn acp_mcp_servers(configured: &[mcp::McpServer], init: &InitializeResponse) -> Vec<McpServer> {
    let http = init.agent_capabilities.mcp_capabilities.http;
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
        })
        .collect()
}

/// Try each advertised auth method in order. Non-interactive methods
/// (API keys read from the agent's env) fail fast when unset;
/// interactive ones (OAuth) block until the user completes the flow in
/// the browser the agent opens.
async fn authenticate(conn: &AgentConn, init: &InitializeResponse, launch: &Launch) -> Result<()> {
    if init.auth_methods.is_empty() {
        return Err(anyhow!(
            "authentication required, but the agent advertises no auth methods"
        ));
    }
    let mut failures = Vec::new();
    for method in &init.auth_methods {
        // Interactive methods (OAuth) block here until the user
        // finishes signing in, so say so rather than looking hung.
        launch.say(&format!(
            "authenticating — {} (finish any sign-in your browser opens)",
            method.name()
        ));
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
    let path = std::env::var("CYDONIA_DEBUG").ok()?;
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
