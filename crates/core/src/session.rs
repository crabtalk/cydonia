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
        CancelNotification, ClientCapabilities, FileSystemCapabilities, InitializeRequest,
        InitializeResponse, NewSessionRequest, NewSessionResponse, PromptRequest,
        ReadTextFileRequest, ReadTextFileResponse, RequestPermissionRequest,
        RequestPermissionResponse, SessionId, SessionNotification, SessionUpdate, StopReason,
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
}

impl Session {
    /// Spawn `entry` over stdio, initialize, open a session, and run `f`
    /// with it. Returns when `f` does; the agent process dies with the
    /// connection.
    pub async fn spawn<T, F>(entry: &settings::Agent, f: F) -> Result<T>
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
                Ok(Self::open(conn, tx, events, f).await)
            })
            .await
            .map_err(|e| anyhow!("ACP connection failed: {e}"))?
    }

    async fn open<T, F>(
        conn: ConnectionTo<Agent>,
        tx: mpsc::UnboundedSender<Event>,
        events: Events,
        f: F,
    ) -> Result<T>
    where
        F: AsyncFnOnce(Session, Events) -> Result<T>,
    {
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

        let cwd = std::env::current_dir().unwrap_or_else(|_| "/".into());
        let response = conn
            .send_request(NewSessionRequest::new(cwd.clone()))
            .block_task()
            .await
            .map_err(|e| anyhow!("session/new failed: {e}"))?;

        f(
            Session {
                conn,
                tx,
                session_id: response.session_id.clone(),
                init,
                response,
                cwd,
            },
            events,
        )
        .await
    }

    /// Send a prompt turn. Its result arrives as [`Event::TurnDone`].
    pub fn prompt(&self, content: &str) -> Result<(), agent_client_protocol::Error> {
        let request = PromptRequest::new(self.session_id.clone(), vec![content.to_owned().into()]);
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
