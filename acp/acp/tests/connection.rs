//! Behaviour that only shows up once both ends are talking: version
//! negotiation, and what a dropped call does to the peer's work.

use cydonia_acp::{Agent, Client, Result, proto};
use std::sync::Arc;
use tokio::sync::mpsc;

/// An agent that answers `initialize` with whatever version it is told to,
/// and reports through `dropped` when a prompt it is serving is cut short.
struct Fake {
    version: proto::ProtocolVersion,
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
    async fn initialize(&self, _: proto::InitializeRequest) -> Result<proto::InitializeResponse> {
        Ok(proto::InitializeResponse {
            protocol_version: self.version,
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
        let _signal = Signal(self.dropped.clone());
        self.started.send(()).unwrap();
        std::future::pending::<()>().await;
        unreachable!()
    }
}

struct Ui;

impl Client for Ui {
    async fn session_update(&self, _: proto::SessionNotification) {}

    async fn request_permission(
        &self,
        _: proto::RequestPermissionRequest,
    ) -> Result<proto::RequestPermissionResponse> {
        Err(proto::Error::method_not_found())
    }
}

fn connect(agent: Fake) -> cydonia_acp::AgentConn {
    let (client_end, agent_end) = tokio::io::duplex(8 * 1024);
    cydonia_acp::serve(agent_end, Arc::new(agent), None);
    cydonia_acp::connect(client_end, Arc::new(Ui), None)
}

#[tokio::test]
async fn requests_after_stdout_has_closed_fail_instead_of_waiting() {
    let conn = cydonia_acp::connect_on(
        tokio::io::BufReader::new(tokio::io::empty()),
        tokio::io::sink(),
        Arc::new(Ui),
        None,
    );
    // Let EOF arrive before the first request, as with a failed agent shim.
    tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    for _ in 0..3 {
        let result = tokio::time::timeout(
            std::time::Duration::from_secs(1),
            conn.initialize(proto::InitializeRequest::new(Default::default())),
        )
        .await;
        assert!(result.expect("a call after EOF must not hang").is_err());
    }
}

#[tokio::test]
async fn a_failed_write_fails_waiting_calls_even_with_stdout_still_open() {
    let (reader, _hold_stdout) = tokio::io::duplex(1024);
    let (writer, closed_stdin) = tokio::io::duplex(1024);
    drop(closed_stdin);
    let conn = cydonia_acp::connect_on(
        tokio::io::BufReader::new(reader),
        writer,
        Arc::new(Ui),
        None,
    );
    let result = tokio::time::timeout(
        std::time::Duration::from_secs(1),
        conn.initialize(proto::InitializeRequest::new(Default::default())),
    )
    .await;
    assert!(result.expect("broken stdin must fail the request").is_err());
}

#[tokio::test]
async fn initialize_refuses_a_version_this_build_cannot_speak() {
    let agent = connect(Fake {
        version: proto::ProtocolVersion(2),
        started: mpsc::unbounded_channel().0,
        dropped: mpsc::unbounded_channel().0,
    });

    let error = agent
        .initialize(proto::InitializeRequest::new(Default::default()))
        .await
        .expect_err("a version we do not speak must not pass silently");
    assert_eq!(error.code, proto::INVALID_REQUEST);

    let agent = connect(Fake {
        version: proto::ProtocolVersion::LATEST,
        started: mpsc::unbounded_channel().0,
        dropped: mpsc::unbounded_channel().0,
    });
    assert!(
        agent
            .initialize(proto::InitializeRequest::new(Default::default()))
            .await
            .is_ok()
    );
}

#[tokio::test]
async fn dropping_a_call_cancels_it_on_the_peer() {
    let (started, mut serving) = mpsc::unbounded_channel();
    let (dropped, mut cancelled) = mpsc::unbounded_channel();
    let agent = connect(Fake {
        version: proto::ProtocolVersion::LATEST,
        started,
        dropped,
    });

    let mut call = Box::pin(agent.prompt(proto::PromptRequest::new("s1", vec!["hello".into()])));
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
