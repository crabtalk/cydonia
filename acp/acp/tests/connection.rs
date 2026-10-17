//! Behaviour that only shows up once both ends are talking: version
//! negotiation, and what a dropped call does to the peer's work.

use cydonia_acp::{Agent, Client, Result, schema};
use std::sync::Arc;
use tokio::sync::mpsc;

/// An agent that answers `initialize` with whatever version it is told to,
/// and reports through `dropped` when a prompt it is serving is cut short.
struct Fake {
    version: schema::ProtocolVersion,
    started: mpsc::UnboundedSender<()>,
    dropped: mpsc::UnboundedSender<()>,
}

/// Fires when the turn is cancelled: cydonia-acp aborts the task serving it, which
/// drops this future mid-await.
struct Signal(mpsc::UnboundedSender<()>);

impl Drop for Signal {
    fn drop(&mut self) {
        let _ = self.0.send(());
    }
}

impl Agent for Fake {
    async fn initialize(&self, _: schema::InitializeRequest) -> Result<schema::InitializeResponse> {
        Ok(schema::InitializeResponse {
            protocol_version: self.version,
            agent_capabilities: Default::default(),
            auth_methods: Vec::new(),
            agent_info: None,
            meta: None,
        })
    }

    async fn new_session(
        &self,
        _: schema::NewSessionRequest,
    ) -> Result<schema::NewSessionResponse> {
        Ok(schema::NewSessionResponse {
            session_id: "s1".into(),
            modes: None,
            config_options: None,
            meta: None,
        })
    }

    async fn prompt(&self, _: schema::PromptRequest) -> Result<schema::PromptResponse> {
        let _signal = Signal(self.dropped.clone());
        self.started.send(()).unwrap();
        std::future::pending::<()>().await;
        unreachable!()
    }
}

struct Ui;

impl Client for Ui {
    async fn session_update(&self, _: schema::SessionNotification) {}

    async fn request_permission(
        &self,
        _: schema::RequestPermissionRequest,
    ) -> Result<schema::RequestPermissionResponse> {
        Err(schema::Error::method_not_found())
    }
}

fn connect(agent: Fake) -> cydonia_acp::AgentConn {
    let (client_end, agent_end) = tokio::io::duplex(8 * 1024);
    cydonia_acp::serve(agent_end, Arc::new(agent), None);
    cydonia_acp::connect(client_end, Arc::new(Ui), None)
}

#[tokio::test]
async fn initialize_refuses_a_version_this_build_cannot_speak() {
    let agent = connect(Fake {
        version: schema::ProtocolVersion(2),
        started: mpsc::unbounded_channel().0,
        dropped: mpsc::unbounded_channel().0,
    });

    let error = agent
        .initialize(schema::InitializeRequest::new(Default::default()))
        .await
        .expect_err("a version we do not speak must not pass silently");
    assert_eq!(error.code, schema::INVALID_REQUEST);

    let agent = connect(Fake {
        version: schema::ProtocolVersion::LATEST,
        started: mpsc::unbounded_channel().0,
        dropped: mpsc::unbounded_channel().0,
    });
    assert!(
        agent
            .initialize(schema::InitializeRequest::new(Default::default()))
            .await
            .is_ok()
    );
}

#[tokio::test]
async fn dropping_a_call_cancels_it_on_the_peer() {
    let (started, mut serving) = mpsc::unbounded_channel();
    let (dropped, mut cancelled) = mpsc::unbounded_channel();
    let agent = connect(Fake {
        version: schema::ProtocolVersion::LATEST,
        started,
        dropped,
    });

    let mut call = Box::pin(agent.prompt(schema::PromptRequest::new("s1", vec!["hello".into()])));
    tokio::select! {
        // Polled first, so the request is on the wire before we walk away.
        biased;
        _ = &mut call => unreachable!("the agent never answers"),
        _ = serving.recv() => {}
    }
    drop(call);

    tokio::time::timeout(std::time::Duration::from_secs(5), cancelled.recv())
        .await
        .expect("the agent should have been told to stop")
        .unwrap();
}
