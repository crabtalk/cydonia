//! cydonia-acp is the client, the reference implementation is the agent.
//!
//! Every method of the pairing crosses the wire: cydonia-acp calls each agent method,
//! and the official agent calls back into cydonia-acp's [`Client`] for each of the
//! client's own. Whatever either side cannot parse fails the test.

use agent_client_protocol as official;
use agent_client_protocol::schema::v1;
use cydonia_acp::{Client, Result, proto};
use std::sync::Arc;

mod support;

use support::{Served, byte_streams, within};

/// A cydonia-acp client that answers everything and records what it was asked.
struct Ui(Served);

impl Client for Ui {
    async fn session_update(&self, _: proto::SessionNotification) {
        self.0.saw("session/update");
    }

    async fn request_permission(
        &self,
        request: proto::RequestPermissionRequest,
    ) -> Result<proto::RequestPermissionResponse> {
        self.0.saw("session/request_permission");
        Ok(proto::RequestPermissionResponse::selected(
            request.options[0].option_id.clone(),
        ))
    }

    async fn read_text_file(
        &self,
        _: proto::ReadTextFileRequest,
    ) -> Result<proto::ReadTextFileResponse> {
        self.0.saw("fs/read_text_file");
        Ok(proto::ReadTextFileResponse {
            content: "hello".into(),
            meta: None,
        })
    }

    async fn write_text_file(
        &self,
        _: proto::WriteTextFileRequest,
    ) -> Result<proto::WriteTextFileResponse> {
        self.0.saw("fs/write_text_file");
        Ok(Default::default())
    }

    async fn create_terminal(
        &self,
        _: proto::CreateTerminalRequest,
    ) -> Result<proto::CreateTerminalResponse> {
        self.0.saw("terminal/create");
        Ok(proto::CreateTerminalResponse {
            terminal_id: "term-1".into(),
            meta: None,
        })
    }

    async fn terminal_output(
        &self,
        _: proto::TerminalOutputRequest,
    ) -> Result<proto::TerminalOutputResponse> {
        self.0.saw("terminal/output");
        Ok(proto::TerminalOutputResponse {
            output: "out".into(),
            truncated: false,
            exit_status: Some(proto::TerminalExitStatus {
                exit_code: Some(0),
                signal: None,
                meta: None,
            }),
            meta: None,
        })
    }

    async fn release_terminal(
        &self,
        _: proto::ReleaseTerminalRequest,
    ) -> Result<proto::ReleaseTerminalResponse> {
        self.0.saw("terminal/release");
        Ok(Default::default())
    }

    async fn wait_for_terminal_exit(
        &self,
        _: proto::WaitForTerminalExitRequest,
    ) -> Result<proto::WaitForTerminalExitResponse> {
        self.0.saw("terminal/wait_for_exit");
        Ok(proto::WaitForTerminalExitResponse {
            exit_status: proto::TerminalExitStatus {
                exit_code: Some(0),
                signal: None,
                meta: None,
            },
            meta: None,
        })
    }

    async fn kill_terminal(
        &self,
        _: proto::KillTerminalRequest,
    ) -> Result<proto::KillTerminalResponse> {
        self.0.saw("terminal/kill");
        Ok(Default::default())
    }

    async fn create_elicitation(
        &self,
        _: proto::CreateElicitationRequest,
    ) -> Result<proto::CreateElicitationResponse> {
        self.0.saw("elicitation/create");
        Ok(proto::CreateElicitationResponse {
            action: proto::ElicitationAction::Decline,
            meta: None,
        })
    }

    async fn complete_elicitation(&self, _: proto::CompleteElicitationNotification) {
        self.0.saw("elicitation/complete");
    }

    async fn connect_mcp(&self, _: proto::ConnectMcpRequest) -> Result<proto::ConnectMcpResponse> {
        self.0.saw("mcp/connect");
        Ok(proto::ConnectMcpResponse {
            connection_id: "mcp-1".into(),
            meta: None,
        })
    }

    async fn message_mcp(&self, _: proto::MessageMcpRequest) -> Result<proto::MessageMcpResponse> {
        self.0.saw("mcp/message");
        Ok(proto::MessageMcpResponse(
            serde_json::json!({"jsonrpc": "2.0", "id": 1, "result": {}}),
        ))
    }

    async fn notify_mcp(&self, _: proto::MessageMcpNotification) {
        self.0.saw("mcp/message (notification)");
    }

    async fn disconnect_mcp(
        &self,
        _: proto::DisconnectMcpRequest,
    ) -> Result<proto::DisconnectMcpResponse> {
        self.0.saw("mcp/disconnect");
        Ok(Default::default())
    }
}

/// The official `mcp/message` response is a raw JSON document.
fn raw_message() -> std::sync::Arc<serde_json::value::RawValue> {
    serde_json::value::RawValue::from_string(r#"{"jsonrpc":"2.0","id":1,"result":{}}"#.into())
        .expect("valid json")
        .into()
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

#[tokio::test(flavor = "multi_thread")]
async fn every_method_of_the_pairing_crosses_the_wire() {
    within(every_method()).await
}

async fn every_method() {
    let (ours, theirs) = tokio::io::duplex(64 * 1024);
    let served = Served::default();

    let agent = official::Agent.builder().name("official-agent");
    let agent = answers!(
        agent,
        v1::AuthenticateRequest,
        v1::AuthenticateResponse::new()
    );
    let agent = answers!(agent, v1::LogoutRequest, v1::LogoutResponse::new());
    let agent = answers!(
        agent,
        v1::NewSessionRequest,
        v1::NewSessionResponse::new("s1")
    );
    let agent = answers!(
        agent,
        v1::LoadSessionRequest,
        v1::LoadSessionResponse::new()
    );
    let agent = answers!(
        agent,
        v1::ResumeSessionRequest,
        v1::ResumeSessionResponse::new()
    );
    let agent = answers!(
        agent,
        v1::ForkSessionRequest,
        v1::ForkSessionResponse::new("s2")
    );
    let agent = answers!(
        agent,
        v1::CloseSessionRequest,
        v1::CloseSessionResponse::new()
    );
    let agent = answers!(
        agent,
        v1::ListSessionsRequest,
        v1::ListSessionsResponse::new(Vec::new())
    );
    let agent = answers!(
        agent,
        v1::DeleteSessionRequest,
        v1::DeleteSessionResponse::new()
    );
    let agent = answers!(
        agent,
        v1::SetSessionModeRequest,
        v1::SetSessionModeResponse::new()
    );
    let agent = answers!(
        agent,
        v1::SetSessionConfigOptionRequest,
        v1::SetSessionConfigOptionResponse::new(Vec::new())
    );
    let agent = answers!(
        agent,
        v1::MessageMcpRequest,
        v1::MessageMcpResponse::new(raw_message())
    );
    let agent = agent
        .on_receive_request(
            async move |request: v1::InitializeRequest, responder, _| {
                responder.respond(v1::InitializeResponse::new(request.protocol_version))
            },
            official::on_receive_request!(),
        )
        .on_receive_notification(
            async move |_: v1::CancelNotification, _| Ok(()),
            official::on_receive_notification!(),
        )
        // The turn is where the agent calls back into the client, so every
        // client-served method runs inside this one handler.
        .on_receive_request(
            async move |request: v1::PromptRequest, responder, connection| {
                // Nested calls must not run on the dispatch loop, or the
                // official SDK cannot read the client's answers.
                let cx = connection.clone();
                connection.spawn(async move {
                    let session = request.session_id;
                    let connection = cx;
                    connection.send_notification(v1::SessionNotification::new(
                        session.clone(),
                        v1::SessionUpdate::AgentMessageChunk(v1::ContentChunk::new(
                            v1::ContentBlock::Text(v1::TextContent::new("pong")),
                        )),
                    ))?;
                    connection
                        .send_request(v1::ReadTextFileRequest::new(session.clone(), "/tmp/a"))
                        .block_task()
                        .await?;
                    connection
                        .send_request(v1::WriteTextFileRequest::new(
                            session.clone(),
                            "/tmp/a",
                            "hi",
                        ))
                        .block_task()
                        .await?;
                    let terminal = connection
                        .send_request(v1::CreateTerminalRequest::new(session.clone(), "true"))
                        .block_task()
                        .await?
                        .terminal_id;
                    connection
                        .send_request(v1::TerminalOutputRequest::new(
                            session.clone(),
                            terminal.clone(),
                        ))
                        .block_task()
                        .await?;
                    connection
                        .send_request(v1::WaitForTerminalExitRequest::new(
                            session.clone(),
                            terminal.clone(),
                        ))
                        .block_task()
                        .await?;
                    connection
                        .send_request(v1::KillTerminalRequest::new(
                            session.clone(),
                            terminal.clone(),
                        ))
                        .block_task()
                        .await?;
                    connection
                        .send_request(v1::ReleaseTerminalRequest::new(session.clone(), terminal))
                        .block_task()
                        .await?;
                    connection
                        .send_request(v1::RequestPermissionRequest::new(
                            session.clone(),
                            v1::ToolCallUpdate::new("t1", Default::default()),
                            vec![v1::PermissionOption::new(
                                "allow",
                                "Allow",
                                v1::PermissionOptionKind::AllowOnce,
                            )],
                        ))
                        .block_task()
                        .await?;
                    connection
                        .send_request(v1::CreateElicitationRequest::new(
                            v1::ElicitationMode::Url(v1::ElicitationUrlMode::new(
                                v1::ElicitationScope::Session(v1::ElicitationSessionScope::new(
                                    session.clone(),
                                )),
                                "e1",
                                "https://example.com",
                            )),
                            "sign in",
                        ))
                        .block_task()
                        .await?;
                    connection.send_notification(v1::CompleteElicitationNotification::new("e1"))?;
                    let connection_id = connection
                        .send_request(v1::ConnectMcpRequest::new("server"))
                        .block_task()
                        .await?
                        .connection_id;
                    connection
                        .send_request(v1::MessageMcpRequest::new(connection_id.clone(), "ping"))
                        .block_task()
                        .await?;
                    connection.send_notification(v1::MessageMcpNotification::new(
                        connection_id.clone(),
                        "note",
                    ))?;
                    connection
                        .send_request(v1::DisconnectMcpRequest::new(connection_id))
                        .block_task()
                        .await?;

                    responder.respond(v1::PromptResponse::new(v1::StopReason::EndTurn))
                })?;
                Ok(())
            },
            official::on_receive_request!(),
        );

    tokio::spawn(async move { agent.connect_to(byte_streams(theirs)).await });

    let agent = cydonia_acp::connect(ours, Arc::new(Ui(served.clone())), None);

    agent
        .initialize(proto::InitializeRequest::new(Default::default()))
        .await
        .expect("initialize");
    agent
        .authenticate(proto::AuthenticateRequest {
            method_id: "oauth".into(),
            meta: None,
        })
        .await
        .expect("authenticate");
    agent.logout().await.expect("logout");

    let session = agent
        .new_session(proto::NewSessionRequest::new("/tmp"))
        .await
        .expect("session/new")
        .session_id;

    agent
        .load_session(proto::LoadSessionRequest::new(session.clone(), "/tmp"))
        .await
        .expect("session/load");
    agent
        .resume_session(proto::ResumeSessionRequest {
            session_id: session.clone(),
            cwd: "/tmp".into(),
            additional_directories: Vec::new(),
            mcp_servers: Vec::new(),
            meta: None,
        })
        .await
        .expect("session/resume");
    agent
        .fork_session(proto::ForkSessionRequest::new(session.clone(), "/tmp"))
        .await
        .expect("session/fork");
    agent
        .list_sessions(proto::ListSessionsRequest {
            cwd: None,
            cursor: None,
            meta: None,
        })
        .await
        .expect("session/list");
    agent
        .set_session_mode(proto::SetSessionModeRequest {
            session_id: session.clone(),
            mode_id: "code".into(),
            meta: None,
        })
        .await
        .expect("session/set_mode");
    agent
        .set_session_config_option(proto::SetSessionConfigOptionRequest {
            session_id: session.clone(),
            config_id: "verbose".into(),
            value: proto::SessionConfigOptionValue::Boolean { value: true },
            meta: None,
        })
        .await
        .expect("session/set_config_option");
    agent
        .message_mcp(proto::MessageMcpRequest {
            connection_id: "mcp-1".into(),
            method: "ping".into(),
            params: None,
            meta: None,
        })
        .await
        .expect("mcp/message");

    agent
        .prompt(proto::PromptRequest::new(
            session.clone(),
            vec!["ping".into()],
        ))
        .await
        .expect("session/prompt");

    agent
        .cancel(proto::CancelNotification {
            session_id: session.clone(),
            meta: None,
        })
        .expect("session/cancel");

    agent
        .delete_session(proto::DeleteSessionRequest {
            session_id: session.clone(),
            meta: None,
        })
        .await
        .expect("session/delete");
    agent
        .close_session(proto::CloseSessionRequest {
            session_id: session,
            meta: None,
        })
        .await
        .expect("session/close");

    served.assert_saw(&[
        "session/update",
        "session/request_permission",
        "fs/read_text_file",
        "fs/write_text_file",
        "terminal/create",
        "terminal/output",
        "terminal/release",
        "terminal/wait_for_exit",
        "terminal/kill",
        "elicitation/create",
        "elicitation/complete",
        "mcp/connect",
        "mcp/message",
        "mcp/message (notification)",
        "mcp/disconnect",
    ]);
}

/// `$/cancel_request`: cydonia-acp sends it when a call is dropped, and the official
/// agent has to keep talking afterwards.
#[tokio::test(flavor = "multi_thread")]
async fn a_dropped_call_leaves_the_official_agent_talking() {
    within(dropped_call()).await
}

async fn dropped_call() {
    let (ours, theirs) = tokio::io::duplex(64 * 1024);
    let agent = official::Agent
        .builder()
        .name("official-agent")
        .on_receive_request(
            async move |request: v1::InitializeRequest, responder, _| {
                responder.respond(v1::InitializeResponse::new(request.protocol_version))
            },
            official::on_receive_request!(),
        )
        .on_receive_request(
            async move |_: v1::PromptRequest, responder, connection| {
                // Parked off the dispatch loop, so the agent stays responsive
                // while the turn it is serving goes unanswered.
                connection.spawn(async move {
                    std::future::pending::<()>().await;
                    drop(responder);
                    Ok(())
                })?;
                Ok(())
            },
            official::on_receive_request!(),
        );
    tokio::spawn(async move { agent.connect_to(byte_streams(theirs)).await });

    let served = Served::default();
    let agent = cydonia_acp::connect(ours, Arc::new(Ui(served)), None);
    agent
        .initialize(proto::InitializeRequest::new(Default::default()))
        .await
        .expect("initialize");

    {
        let call = agent.prompt(proto::PromptRequest::new("s1", vec!["ping".into()]));
        let mut call = Box::pin(call);
        tokio::select! {
            biased;
            _ = &mut call => panic!("the agent never answers"),
            () = tokio::task::yield_now() => {}
        }
    }

    // The cancel we just sent must not have upset the agent's parser.
    agent
        .initialize(proto::InitializeRequest::new(Default::default()))
        .await
        .expect("the connection still works after $/cancel_request");
}
