//! cydonia-acp is the agent, the reference implementation is the client.
//!
//! The mirror of [`acp_as_client`](../acp_as_client.rs): the official client
//! calls every agent method, and cydonia-acp's agent calls back into it for every
//! client method.

use agent_client_protocol as official;
use agent_client_protocol::schema::{ProtocolVersion, v1};
use cydonia_acp::{Agent, ClientConn, Result, proto};
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;

mod support;

use support::{Served, byte_streams, within};

/// A cydonia-acp agent that serves everything and, during a turn, calls back into the
/// client for every method the client serves.
struct Echo {
    client: Mutex<Option<ClientConn>>,
    served: Served,
}

impl Echo {
    fn client(&self) -> ClientConn {
        self.client
            .lock()
            .expect("poisoned")
            .clone()
            .expect("connected")
    }
}

impl Agent for Echo {
    async fn initialize(
        &self,
        request: proto::InitializeRequest,
    ) -> Result<proto::InitializeResponse> {
        self.served.saw("initialize");
        Ok(proto::InitializeResponse {
            protocol_version: request.protocol_version,
            agent_capabilities: Default::default(),
            auth_methods: vec![proto::AuthMethod::Agent(proto::AuthMethodAgent {
                id: "oauth".into(),
                name: "Sign in".into(),
                description: None,
                meta: None,
            })],
            agent_info: Some(proto::Implementation {
                name: "echo".into(),
                version: "0.1.0".into(),
                title: None,
                meta: None,
            }),
            meta: None,
        })
    }

    async fn authenticate(
        &self,
        _: proto::AuthenticateRequest,
    ) -> Result<proto::AuthenticateResponse> {
        self.served.saw("authenticate");
        Ok(Default::default())
    }

    async fn logout(&self, _: proto::LogoutRequest) -> Result<proto::LogoutResponse> {
        self.served.saw("logout");
        Ok(Default::default())
    }

    async fn new_session(&self, _: proto::NewSessionRequest) -> Result<proto::NewSessionResponse> {
        self.served.saw("session/new");
        Ok(proto::NewSessionResponse {
            session_id: "s1".into(),
            modes: None,
            config_options: None,
            meta: None,
        })
    }

    async fn load_session(
        &self,
        _: proto::LoadSessionRequest,
    ) -> Result<proto::LoadSessionResponse> {
        self.served.saw("session/load");
        Ok(Default::default())
    }

    async fn resume_session(
        &self,
        _: proto::ResumeSessionRequest,
    ) -> Result<proto::ResumeSessionResponse> {
        self.served.saw("session/resume");
        Ok(Default::default())
    }

    async fn fork_session(
        &self,
        _: proto::ForkSessionRequest,
    ) -> Result<proto::ForkSessionResponse> {
        self.served.saw("session/fork");
        Ok(proto::ForkSessionResponse {
            session_id: "s2".into(),
            modes: None,
            config_options: None,
            meta: None,
        })
    }

    async fn close_session(
        &self,
        _: proto::CloseSessionRequest,
    ) -> Result<proto::CloseSessionResponse> {
        self.served.saw("session/close");
        Ok(Default::default())
    }

    async fn list_sessions(
        &self,
        _: proto::ListSessionsRequest,
    ) -> Result<proto::ListSessionsResponse> {
        self.served.saw("session/list");
        Ok(proto::ListSessionsResponse {
            sessions: vec![proto::SessionInfo {
                session_id: "s1".into(),
                cwd: "/tmp".into(),
                additional_directories: Vec::new(),
                title: None,
                updated_at: None,
                meta: None,
            }],
            next_cursor: None,
            meta: None,
        })
    }

    async fn delete_session(
        &self,
        _: proto::DeleteSessionRequest,
    ) -> Result<proto::DeleteSessionResponse> {
        self.served.saw("session/delete");
        Ok(Default::default())
    }

    async fn set_session_mode(
        &self,
        _: proto::SetSessionModeRequest,
    ) -> Result<proto::SetSessionModeResponse> {
        self.served.saw("session/set_mode");
        Ok(Default::default())
    }

    async fn set_session_config_option(
        &self,
        _: proto::SetSessionConfigOptionRequest,
    ) -> Result<proto::SetSessionConfigOptionResponse> {
        self.served.saw("session/set_config_option");
        Ok(Default::default())
    }

    async fn message_mcp(&self, _: proto::MessageMcpRequest) -> Result<proto::MessageMcpResponse> {
        self.served.saw("mcp/message");
        Ok(proto::MessageMcpResponse(
            serde_json::json!({"jsonrpc": "2.0", "id": 1, "result": {}}),
        ))
    }

    async fn notify_mcp(&self, _: proto::MessageMcpNotification) {
        self.served.saw("mcp/message (notification)");
    }

    async fn cancel(&self, _: proto::CancelNotification) {
        self.served.saw("session/cancel");
    }

    /// The turn is where the agent calls the client, so every client-served
    /// method runs from here.
    async fn prompt(&self, request: proto::PromptRequest) -> Result<proto::PromptResponse> {
        self.served.saw("session/prompt");
        let client = self.client();
        let session = request.session_id;

        client.session_update(proto::SessionNotification {
            session_id: session.clone(),
            update: proto::SessionUpdate::AgentMessageChunk(proto::ContentChunk {
                content: "pong".into(),
                message_id: None,
                meta: None,
            }),
            meta: None,
        })?;
        client
            .read_text_file(proto::ReadTextFileRequest {
                session_id: session.clone(),
                path: "/tmp/a".into(),
                line: None,
                limit: None,
                meta: None,
            })
            .await?;
        client
            .write_text_file(proto::WriteTextFileRequest {
                session_id: session.clone(),
                path: "/tmp/a".into(),
                content: "hi".into(),
                meta: None,
            })
            .await?;
        let terminal = client
            .create_terminal(proto::CreateTerminalRequest {
                session_id: session.clone(),
                command: "true".into(),
                args: Vec::new(),
                env: Vec::new(),
                cwd: None,
                output_byte_limit: None,
                meta: None,
            })
            .await?
            .terminal_id;
        client
            .terminal_output(proto::TerminalOutputRequest {
                session_id: session.clone(),
                terminal_id: terminal.clone(),
                meta: None,
            })
            .await?;
        client
            .wait_for_terminal_exit(proto::WaitForTerminalExitRequest {
                session_id: session.clone(),
                terminal_id: terminal.clone(),
                meta: None,
            })
            .await?;
        client
            .kill_terminal(proto::KillTerminalRequest {
                session_id: session.clone(),
                terminal_id: terminal.clone(),
                meta: None,
            })
            .await?;
        client
            .release_terminal(proto::ReleaseTerminalRequest {
                session_id: session.clone(),
                terminal_id: terminal,
                meta: None,
            })
            .await?;
        client
            .request_permission(proto::RequestPermissionRequest {
                session_id: session.clone(),
                tool_call: proto::ToolCall::new("t1", "read").into(),
                options: vec![proto::PermissionOption {
                    option_id: "allow".into(),
                    name: "Allow".into(),
                    kind: proto::PermissionOptionKind::AllowOnce,
                    meta: None,
                }],
                meta: None,
            })
            .await?;
        client
            .create_elicitation(proto::CreateElicitationRequest {
                message: "sign in".into(),
                mode: proto::ElicitationMode::Url(proto::ElicitationUrlMode {
                    scope: proto::ElicitationScope::Session(proto::ElicitationSessionScope {
                        session_id: session.clone(),
                        tool_call_id: None,
                    }),
                    elicitation_id: "e1".into(),
                    url: "https://example.com".into(),
                }),
                meta: None,
            })
            .await?;
        client.complete_elicitation(proto::CompleteElicitationNotification {
            elicitation_id: "e1".into(),
            meta: None,
        })?;
        let connection = client
            .connect_mcp(proto::ConnectMcpRequest {
                server_id: "server".into(),
                meta: None,
            })
            .await?
            .connection_id;
        client
            .message_mcp(proto::MessageMcpRequest {
                connection_id: connection.clone(),
                method: "ping".into(),
                params: None,
                meta: None,
            })
            .await?;
        client.notify_mcp(proto::MessageMcpNotification {
            connection_id: connection.clone(),
            method: "note".into(),
            params: None,
            meta: None,
        })?;
        client
            .disconnect_mcp(proto::DisconnectMcpRequest {
                connection_id: connection,
                meta: None,
            })
            .await?;

        Ok(proto::PromptResponse::new(proto::StopReason::EndTurn))
    }
}

/// Answers `request` with `response`, ignoring what was asked.
macro_rules! answers {
    ($builder:expr, $request:ty, $response:expr) => {
        $builder.on_receive_request(
            async move |_: $request, responder, _| responder.respond($response),
            official::on_receive_request!(),
        )
    };
}

fn raw_message() -> std::sync::Arc<serde_json::value::RawValue> {
    serde_json::value::RawValue::from_string(r#"{"jsonrpc":"2.0","id":1,"result":{}}"#.into())
        .expect("valid json")
        .into()
}

#[tokio::test(flavor = "multi_thread")]
async fn every_method_of_the_pairing_crosses_the_wire() {
    within(every_method()).await
}

async fn every_method() {
    let (ours, theirs) = tokio::io::duplex(64 * 1024);

    let agent_side = Served::default();
    let client_side = Served::default();

    let echo = Arc::new(Echo {
        client: Mutex::new(None),
        served: agent_side.clone(),
    });
    let conn = cydonia_acp::serve(ours, Arc::clone(&echo), None);
    *echo.client.lock().expect("poisoned") = Some(conn);

    let client = official::Client.builder().name("official-client");
    let client = answers!(
        client,
        v1::ReadTextFileRequest,
        v1::ReadTextFileResponse::new("hello")
    );
    let client = answers!(
        client,
        v1::WriteTextFileRequest,
        v1::WriteTextFileResponse::new()
    );
    let client = answers!(
        client,
        v1::CreateTerminalRequest,
        v1::CreateTerminalResponse::new("term-1")
    );
    let client = answers!(
        client,
        v1::TerminalOutputRequest,
        v1::TerminalOutputResponse::new("out", false)
    );
    let client = answers!(
        client,
        v1::WaitForTerminalExitRequest,
        v1::WaitForTerminalExitResponse::new(v1::TerminalExitStatus::new())
    );
    let client = answers!(
        client,
        v1::KillTerminalRequest,
        v1::KillTerminalResponse::new()
    );
    let client = answers!(
        client,
        v1::ReleaseTerminalRequest,
        v1::ReleaseTerminalResponse::new()
    );
    let client = answers!(
        client,
        v1::CreateElicitationRequest,
        v1::CreateElicitationResponse::new(v1::ElicitationAction::Decline)
    );
    let client = answers!(
        client,
        v1::ConnectMcpRequest,
        v1::ConnectMcpResponse::new("mcp-1")
    );
    let client = answers!(
        client,
        v1::MessageMcpRequest,
        v1::MessageMcpResponse::new(raw_message())
    );
    let client = answers!(
        client,
        v1::DisconnectMcpRequest,
        v1::DisconnectMcpResponse::new()
    );

    let seen = client_side.clone();
    let permissions = client_side.clone();
    let notifications = client_side.clone();
    let elicitations = client_side.clone();
    let client = client
        .on_receive_request(
            async move |request: v1::RequestPermissionRequest, responder, _| {
                permissions.saw("session/request_permission");
                responder.respond(v1::RequestPermissionResponse::new(
                    v1::RequestPermissionOutcome::Selected(v1::SelectedPermissionOutcome::new(
                        request.options[0].option_id.clone(),
                    )),
                ))
            },
            official::on_receive_request!(),
        )
        .on_receive_notification(
            async move |_: v1::SessionNotification, _| {
                notifications.saw("session/update");
                Ok(())
            },
            official::on_receive_notification!(),
        )
        .on_receive_notification(
            async move |_: v1::CompleteElicitationNotification, _| {
                elicitations.saw("elicitation/complete");
                Ok(())
            },
            official::on_receive_notification!(),
        )
        .on_receive_notification(
            async move |_: v1::MessageMcpNotification, _| {
                seen.saw("mcp/message (notification)");
                Ok(())
            },
            official::on_receive_notification!(),
        );

    client
        .connect_with(
            byte_streams(theirs),
            |connection: official::ConnectionTo<official::Agent>| async move {
                connection
                    .send_request(v1::InitializeRequest::new(ProtocolVersion::V1))
                    .block_task()
                    .await?;
                connection
                    .send_request(v1::AuthenticateRequest::new("oauth"))
                    .block_task()
                    .await?;
                connection
                    .send_request(v1::LogoutRequest::new())
                    .block_task()
                    .await?;
                let session = connection
                    .send_request(v1::NewSessionRequest::new("/tmp"))
                    .block_task()
                    .await?
                    .session_id;
                connection
                    .send_request(v1::LoadSessionRequest::new(session.clone(), "/tmp"))
                    .block_task()
                    .await?;
                connection
                    .send_request(v1::ResumeSessionRequest::new(session.clone(), "/tmp"))
                    .block_task()
                    .await?;
                connection
                    .send_request(v1::ForkSessionRequest::new(session.clone(), "/tmp"))
                    .block_task()
                    .await?;
                connection
                    .send_request(v1::ListSessionsRequest::new())
                    .block_task()
                    .await?;
                connection
                    .send_request(v1::SetSessionModeRequest::new(session.clone(), "code"))
                    .block_task()
                    .await?;
                connection
                    .send_request(v1::SetSessionConfigOptionRequest::new(
                        session.clone(),
                        "verbose",
                        v1::SessionConfigOptionValue::Boolean { value: true },
                    ))
                    .block_task()
                    .await?;
                connection
                    .send_request(v1::MessageMcpRequest::new("mcp-1", "ping"))
                    .block_task()
                    .await?;
                connection
                    .send_request(v1::PromptRequest::new(
                        session.clone(),
                        vec![v1::ContentBlock::Text(v1::TextContent::new("ping"))],
                    ))
                    .block_task()
                    .await?;
                connection.send_notification(v1::CancelNotification::new(session.clone()))?;
                connection
                    .send_request(v1::DeleteSessionRequest::new(session.clone()))
                    .block_task()
                    .await?;
                connection
                    .send_request(v1::CloseSessionRequest::new(session))
                    .block_task()
                    .await?;
                Ok(())
            },
        )
        .await
        .expect("the official client ran every method");

    agent_side.assert_saw(&[
        "initialize",
        "authenticate",
        "logout",
        "session/new",
        "session/load",
        "session/resume",
        "session/fork",
        "session/list",
        "session/set_mode",
        "session/set_config_option",
        "session/prompt",
        "session/cancel",
        "session/delete",
        "session/close",
        "mcp/message",
    ]);
    client_side.assert_saw(&[
        "session/update",
        "session/request_permission",
        "elicitation/complete",
        "mcp/message (notification)",
    ]);
}

/// A cydonia-acp agent whose turn never ends, and which reports when the task
/// serving it is torn down.
struct Sleeper {
    started: mpsc::UnboundedSender<()>,
    aborted: mpsc::UnboundedSender<()>,
}

struct Signal(mpsc::UnboundedSender<()>);

impl Drop for Signal {
    fn drop(&mut self) {
        let _ = self.0.send(());
    }
}

impl Agent for Sleeper {
    async fn initialize(
        &self,
        request: proto::InitializeRequest,
    ) -> Result<proto::InitializeResponse> {
        Ok(proto::InitializeResponse {
            protocol_version: request.protocol_version,
            agent_capabilities: Default::default(),
            auth_methods: Vec::new(),
            agent_info: None,
            meta: None,
        })
    }

    async fn new_session(&self, _: proto::NewSessionRequest) -> Result<proto::NewSessionResponse> {
        Ok(proto::NewSessionResponse {
            session_id: "s1".into(),
            modes: None,
            config_options: None,
            meta: None,
        })
    }

    async fn prompt(&self, _: proto::PromptRequest) -> Result<proto::PromptResponse> {
        let _signal = Signal(self.aborted.clone());
        let _ = self.started.send(());
        std::future::pending::<()>().await;
        unreachable!()
    }
}

/// `$/cancel_request` inbound: the official client drops a call, and cydonia-acp has
/// to stop serving it.
#[tokio::test(flavor = "multi_thread")]
async fn a_dropped_official_call_stops_the_cacp_agent() {
    within(dropped_call()).await
}

async fn dropped_call() {
    let (ours, theirs) = tokio::io::duplex(64 * 1024);
    let (started, mut serving) = mpsc::unbounded_channel();
    let (aborted, mut torn_down) = mpsc::unbounded_channel();

    cydonia_acp::serve(ours, Arc::new(Sleeper { started, aborted }), None);

    official::Client
        .builder()
        .name("official-client")
        .connect_with(
            byte_streams(theirs),
            |connection: official::ConnectionTo<official::Agent>| async move {
                connection
                    .send_request(v1::InitializeRequest::new(ProtocolVersion::V1))
                    .block_task()
                    .await?;
                let session = connection
                    .send_request(v1::NewSessionRequest::new("/tmp"))
                    .block_task()
                    .await?
                    .session_id;

                let call = connection.send_request(v1::PromptRequest::new(
                    session,
                    vec![v1::ContentBlock::Text(v1::TextContent::new("ping"))],
                ));
                serving.recv().await.expect("the agent started the turn");
                drop(call);

                torn_down.recv().await.expect("the turn was torn down");
                Ok(())
            },
        )
        .await
        .expect("the official client cancelled its call");
}
