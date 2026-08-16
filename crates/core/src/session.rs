//! One ACP session over a spawned agent subprocess.
//!
//! Wraps the SDK's connection setup and the traps we hit wiring it:
//! - All agent-side events flow through ONE channel. The producers run
//!   sequentially on the SDK's dispatch task, so a single channel preserves
//!   the exact wire order (a turn's final updates arrive before its result —
//!   separate channels lose that).
//! - The prompt response hook never returns an error: a hook error tears the
//!   whole connection down, and an agent is allowed to fail a single turn.
//! - Unhandled session-scoped requests must be declined explicitly. The
//!   role's default parks them waiting for an `ActiveSession` handler we
//!   never register — the agent hangs.

use crate::settings;
use agent_client_protocol::schema::{
    ProtocolVersion,
    v1::{
        AuthenticateRequest, CancelNotification, ClientCapabilities, ContentBlock, EnvVariable,
        FileSystemCapabilities, InitializeRequest, InitializeResponse, LoadSessionRequest,
        McpServer, McpServerStdio, NewSessionRequest, NewSessionResponse, PromptRequest,
        ReadTextFileRequest, ReadTextFileResponse, RequestPermissionRequest,
        RequestPermissionResponse, SessionConfigOptionValue, SessionId, SessionNotification,
        SessionUpdate, SetSessionConfigOptionRequest, SetSessionModeRequest, StopReason,
        WriteTextFileRequest, WriteTextFileResponse,
    },
};
use agent_client_protocol::{
    AcpAgent, AcpAgentConfig, Agent, Client, ConnectionTo, Dispatch, Handled, Responder,
};
use anyhow::{Result, anyhow};
use futures::channel::mpsc;
use std::path::PathBuf;

/// Everything the agent side feeds into the frontend, in wire order.
pub enum Event {
    Update(SessionUpdate),
    /// The agent asks the user to authorize a tool call. The responder must
    /// be answered exactly once (`Cancelled` if the turn is cancelled).
    Permission(
        RequestPermissionRequest,
        Responder<RequestPermissionResponse>,
    ),
    /// The prompt turn settled: its stop reason, or the agent's error.
    TurnDone(Result<StopReason, agent_client_protocol::Error>),
}

/// The frontend's receiving end. Handed to the `spawn` closure separately
/// from [`Session`] so an event loop can poll it while the session handle
/// stays borrowable for `prompt`/`cancel`.
pub type Events = mpsc::UnboundedReceiver<Event>;

/// A live session: the connection and its identity.
pub struct Session {
    conn: ConnectionTo<Agent>,
    tx: mpsc::UnboundedSender<Event>,
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
    /// Spawn `entry` over stdio, initialize, open a session, and run `f`
    /// with it. Returns when `f` does; the agent process dies with the
    /// connection.
    ///
    /// With [`Launch::previous`] set and the agent capable,
    /// `session/load` replays that session's history instead of
    /// starting fresh; a failed load (stale id, agent restart) falls
    /// back to a new session.
    pub async fn spawn<T, F>(entry: &settings::Agent, launch: Launch, f: F) -> Result<T>
    where
        F: AsyncFnOnce(Session, Events) -> Result<T>,
    {
        let mut config = AcpAgentConfig::new(&entry.command);
        for arg in &entry.args {
            config = config.arg(arg);
        }
        for (key, value) in &entry.env {
            config = config.env(key, value);
        }

        let mcp_servers: Vec<McpServer> = entry
            .mcp_servers
            .iter()
            .map(|server| {
                let mut stdio = McpServerStdio::new(server.name.clone(), server.command.clone());
                stdio.args = server.args.clone();
                stdio.env = server
                    .env
                    .iter()
                    .map(|(name, value)| EnvVariable::new(name.clone(), value.clone()))
                    .collect();
                McpServer::Stdio(stdio)
            })
            .collect();

        let (tx, events) = mpsc::unbounded::<Event>();
        let notify_tx = tx.clone();
        let permission_tx = tx.clone();

        Client
            .builder()
            .on_receive_notification(
                async move |notification: SessionNotification, _cx| {
                    let _ = notify_tx.unbounded_send(Event::Update(notification.update));
                    Ok(())
                },
                agent_client_protocol::on_receive_notification!(),
            )
            .on_receive_request(
                async move |request: RequestPermissionRequest, responder, _conn| {
                    let _ = permission_tx.unbounded_send(Event::Permission(request, responder));
                    Ok(())
                },
                agent_client_protocol::on_receive_request!(),
            )
            .on_receive_request(
                async move |request: ReadTextFileRequest, responder, _conn| {
                    responder.respond_with_result(read_text_file(&request))
                },
                agent_client_protocol::on_receive_request!(),
            )
            .on_receive_request(
                async move |request: WriteTextFileRequest, responder, _conn| {
                    responder.respond_with_result(
                        std::fs::write(&request.path, &request.content)
                            .map(|()| WriteTextFileResponse::new())
                            .map_err(|e| io_error(&request.path, &e)),
                    )
                },
                agent_client_protocol::on_receive_request!(),
            )
            // Catch-all AFTER the typed handlers: decline what we don't
            // serve (terminal/*, elicitation) instead of letting it park.
            .on_receive_dispatch(
                async move |dispatch: Dispatch, _conn| match dispatch {
                    Dispatch::Request(request, responder) => {
                        let method = request.method().to_owned();
                        responder.respond_with_error(
                            agent_client_protocol::Error::method_not_found().data(method),
                        )?;
                        Ok(Handled::Yes)
                    }
                    other => Ok(Handled::No {
                        message: other,
                        retry: false,
                    }),
                },
                agent_client_protocol::on_receive_dispatch!(),
            )
            .connect_with(debuggable(AcpAgent::new(config)), async |conn| {
                Ok(Self::open(conn, tx, events, launch, mcp_servers, f).await)
            })
            .await
            .map_err(|e| anyhow!("ACP connection failed: {e}"))?
    }

    async fn open<T, F>(
        conn: ConnectionTo<Agent>,
        tx: mpsc::UnboundedSender<Event>,
        events: Events,
        launch: Launch,
        mcp_servers: Vec<McpServer>,
        f: F,
    ) -> Result<T>
    where
        F: AsyncFnOnce(Session, Events) -> Result<T>,
    {
        let cwd = launch.cwd.clone();
        let init = conn
            .send_request(
                InitializeRequest::new(ProtocolVersion::V1).client_capabilities(
                    ClientCapabilities::new().fs(FileSystemCapabilities::new()
                        .read_text_file(true)
                        .write_text_file(true)),
                ),
            )
            .block_task()
            .await
            .map_err(|e| anyhow!("initialize failed: {e}"))?;

        let mut loaded = false;
        let mut response = None;
        if let Some(id) = launch
            .previous
            .clone()
            .filter(|_| init.agent_capabilities.load_session)
        {
            let load = || {
                LoadSessionRequest::new(id.clone(), cwd.clone()).mcp_servers(mcp_servers.clone())
            };
            let result = match conn.send_request(load()).block_task().await {
                Err(e) if e.code == agent_client_protocol::Error::auth_required().code => {
                    authenticate(&conn, &init, &launch).await?;
                    conn.send_request(load()).block_task().await
                }
                other => other,
            };
            // A failed load (stale id, agent state gone) falls through
            // to a fresh session rather than failing the launch.
            if let Ok(load_response) = result {
                let mut restored = NewSessionResponse::new(id);
                restored.modes = load_response.modes;
                restored.config_options = load_response.config_options;
                response = Some(restored);
                loaded = true;
            }
        }

        let response = match response {
            Some(response) => response,
            None => {
                let new_session =
                    || NewSessionRequest::new(cwd.clone()).mcp_servers(mcp_servers.clone());
                match conn.send_request(new_session()).block_task().await {
                    Ok(response) => response,
                    Err(e) if e.code == agent_client_protocol::Error::auth_required().code => {
                        authenticate(&conn, &init, &launch).await?;
                        conn.send_request(new_session())
                            .block_task()
                            .await
                            .map_err(|e| anyhow!("session/new failed after authentication: {e}"))?
                    }
                    Err(e) => return Err(anyhow!("session/new failed: {e}")),
                }
            }
        };

        f(
            Session {
                conn,
                tx,
                session_id: response.session_id.clone(),
                init,
                response,
                cwd,
                loaded,
            },
            events,
        )
        .await
    }

    /// Send a prompt turn. Its result arrives as [`Event::TurnDone`].
    pub fn prompt(&self, content: &str) -> Result<(), agent_client_protocol::Error> {
        self.prompt_blocks(vec![content.to_owned().into()])
    }

    /// Send a prompt turn with explicit content blocks (text plus
    /// embedded resources). Same result path as [`Self::prompt`].
    pub fn prompt_blocks(
        &self,
        blocks: Vec<ContentBlock>,
    ) -> Result<(), agent_client_protocol::Error> {
        let request = PromptRequest::new(self.session_id.clone(), blocks);
        let tx = self.tx.clone();
        self.conn
            .send_request(request)
            .on_receiving_result(move |result| {
                let _ =
                    tx.unbounded_send(Event::TurnDone(result.map(|response| response.stop_reason)));
                async { Ok(()) }
            })
    }

    /// Cancel the in-flight turn (`session/cancel`). The turn still ends
    /// with a [`Event::TurnDone`] carrying `StopReason::Cancelled`. Pending
    /// permission responders are the frontend's to answer `Cancelled`.
    pub fn cancel(&self) -> Result<(), agent_client_protocol::Error> {
        self.conn
            .send_notification(CancelNotification::new(self.session_id.clone()))
    }

    /// Switch the session mode (`session/set_mode`). Fire-and-forget:
    /// frontends validate the id against `response.modes` up front, and
    /// the agent's `CurrentModeUpdate` is the confirmation.
    pub fn set_mode(&self, mode_id: &str) -> Result<(), agent_client_protocol::Error> {
        self.conn
            .send_request(SetSessionModeRequest::new(
                self.session_id.clone(),
                mode_id.to_owned(),
            ))
            .on_receiving_result(move |_| async { Ok(()) })
    }

    /// Set a session config option (`session/set_config_option`).
    /// Fire-and-forget like [`Self::set_mode`]: frontends validate
    /// against `response.config_options`, and the agent's
    /// `ConfigOptionUpdate` is the confirmation.
    pub fn set_config_option(
        &self,
        config_id: &str,
        value: SessionConfigOptionValue,
    ) -> Result<(), agent_client_protocol::Error> {
        self.conn
            .send_request(SetSessionConfigOptionRequest::new(
                self.session_id.clone(),
                config_id.to_owned(),
                value,
            ))
            .on_receiving_result(move |_| async { Ok(()) })
    }
}

// A GPUI (or any multi-threaded) frontend holds `Session` in its UI state
// and moves `Event` — responder included — across executor threads.
const _: () = {
    const fn assert_send<T: Send>() {}
    assert_send::<Session>();
    assert_send::<Event>();
};

/// Try each advertised auth method in order. Non-interactive methods
/// (API keys read from the agent's env) fail fast when unset;
/// interactive ones (OAuth) block until the user completes the flow in
/// the browser the agent opens.
async fn authenticate(
    conn: &ConnectionTo<Agent>,
    init: &InitializeResponse,
    launch: &Launch,
) -> Result<()> {
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
        match conn
            .send_request(AuthenticateRequest::new(method.id().clone()))
            .block_task()
            .await
        {
            Ok(_) => return Ok(()),
            Err(e) => failures.push(format!("{}: {}", method.name(), error_text(&e))),
        }
    }
    Err(anyhow!("authentication failed — {}", failures.join("; ")))
}

/// One-line rendering of a JSON-RPC error (`Display` dumps a JSON blob).
pub fn error_text(e: &agent_client_protocol::Error) -> String {
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
fn read_text_file(
    request: &ReadTextFileRequest,
) -> Result<ReadTextFileResponse, agent_client_protocol::Error> {
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
    Ok(ReadTextFileResponse::new(content))
}

fn io_error(path: &std::path::Path, e: &std::io::Error) -> agent_client_protocol::Error {
    agent_client_protocol::Error::internal_error().data(format!("{}: {e}", path.display()))
}

/// With `CYDONIA_DEBUG=<path>` set, append every JSON-RPC line to that file.
fn debuggable(agent: AcpAgent) -> AcpAgent {
    let Ok(path) = std::env::var("CYDONIA_DEBUG") else {
        return agent;
    };
    agent.with_debug(move |line, direction| {
        use std::io::Write;
        if let Ok(mut f) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
        {
            let _ = writeln!(f, "{direction:?}: {line}");
        }
    })
}
