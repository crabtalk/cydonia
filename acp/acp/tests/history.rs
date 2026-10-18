use cydonia_acp::{
    Agent, Client, Result,
    client::{HistoryEntry, HistoryRole},
    proto,
};
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicUsize, Ordering},
};
use tokio::sync::mpsc;

struct Fake {
    opens: Arc<AtomicUsize>,
    prompts: mpsc::UnboundedSender<proto::PromptRequest>,
    fail: Arc<AtomicBool>,
}

impl Agent for Fake {
    async fn initialize(&self, _: proto::InitializeRequest) -> Result<proto::InitializeResponse> {
        unreachable!()
    }

    async fn new_session(&self, _: proto::NewSessionRequest) -> Result<proto::NewSessionResponse> {
        self.opens.fetch_add(1, Ordering::SeqCst);
        Ok(proto::NewSessionResponse {
            session_id: "fork".into(),
            modes: None,
            config_options: None,
            meta: None,
        })
    }

    async fn prompt(&self, request: proto::PromptRequest) -> Result<proto::PromptResponse> {
        self.prompts.send(request).unwrap();
        if self.fail.swap(false, Ordering::SeqCst) {
            return Err(proto::Error::invalid_params());
        }
        Ok(proto::PromptResponse::new(proto::StopReason::EndTurn))
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

fn setup() -> (
    cydonia_acp::AgentConn,
    mpsc::UnboundedReceiver<proto::PromptRequest>,
    Arc<AtomicUsize>,
    Arc<AtomicBool>,
) {
    let (tx, rx) = mpsc::unbounded_channel();
    let opens = Arc::new(AtomicUsize::new(0));
    let fail = Arc::new(AtomicBool::new(false));
    let (client, agent) = tokio::io::duplex(8192);
    cydonia_acp::serve(
        agent,
        Arc::new(Fake {
            opens: opens.clone(),
            prompts: tx,
            fail: fail.clone(),
        }),
        None,
    );
    (
        cydonia_acp::connect(client, Arc::new(Ui), None),
        rx,
        opens,
        fail,
    )
}

fn history() -> Vec<HistoryEntry> {
    vec![
        HistoryEntry {
            role: HistoryRole::User,
            content: vec!["earlier question".into()],
        },
        HistoryEntry {
            role: HistoryRole::Agent,
            content: vec!["earlier answer".into()],
        },
        HistoryEntry {
            role: HistoryRole::User,
            content: vec!["excluded message".into()],
        },
    ]
}

#[tokio::test]
async fn fork_stays_idle_then_sends_only_the_selected_prefix_once() {
    let (agent, mut prompts, opens, _) = setup();
    let saved = history();
    let mut fork = agent
        .fork_session_from_history(proto::NewSessionRequest::new("/tmp"), &saved, 2)
        .await
        .unwrap();
    assert_eq!(opens.load(Ordering::SeqCst), 1);
    assert!(prompts.try_recv().is_err());
    assert_eq!(fork.pending_history(), &saved[..2]);
    let mut request = proto::PromptRequest::new(
        fork.session().session_id.clone(),
        vec!["edited question".into()],
    );
    request.meta = Some(serde_json::from_value(serde_json::json!({"trace": "kept"})).unwrap());
    fork.prompt(request.clone()).await.unwrap();
    let sent = prompts.recv().await.unwrap();
    assert_eq!(sent.session_id, request.session_id);
    assert_eq!(sent.meta, request.meta);
    assert_eq!(sent.prompt[2], saved[0].content[0]);
    assert_eq!(sent.prompt[4], saved[1].content[0]);
    assert_eq!(sent.prompt.last(), request.prompt.last());
    assert!(!sent.prompt.contains(&saved[2].content[0]));
    assert!(fork.pending_history().is_empty());
    fork.prompt(request.clone()).await.unwrap();
    assert_eq!(prompts.recv().await.unwrap(), request);
}

#[tokio::test]
async fn empty_fork_and_invalid_cutoff_do_not_import_history() {
    let (agent, mut prompts, opens, _) = setup();
    assert!(
        agent
            .fork_session_from_history(proto::NewSessionRequest::new("/tmp"), &history(), 4)
            .await
            .is_err()
    );
    assert_eq!(opens.load(Ordering::SeqCst), 0);
    let mut fork = agent
        .fork_session_from_history(proto::NewSessionRequest::new("/tmp"), &history(), 0)
        .await
        .unwrap();
    let request = proto::PromptRequest::new("fork", vec!["new question".into()]);
    fork.prompt(request.clone()).await.unwrap();
    assert_eq!(prompts.recv().await.unwrap(), request);
}

#[tokio::test]
async fn errors_keep_pending_history_and_wrong_session_is_rejected_locally() {
    let (agent, mut prompts, _, fail) = setup();
    let saved = history();
    let mut fork = agent
        .fork_session_from_history(proto::NewSessionRequest::new("/tmp"), &saved, saved.len())
        .await
        .unwrap();
    assert!(
        fork.prompt(proto::PromptRequest::new("original", vec![]))
            .await
            .is_err()
    );
    assert!(prompts.try_recv().is_err());
    fail.store(true, Ordering::SeqCst);
    assert!(
        fork.prompt(proto::PromptRequest::new("fork", vec!["retry".into()]))
            .await
            .is_err()
    );
    assert_eq!(fork.pending_history(), saved);
    let failed = prompts.recv().await.unwrap();
    fork.prompt(proto::PromptRequest::new("fork", vec!["retry".into()]))
        .await
        .unwrap();
    assert_eq!(prompts.recv().await.unwrap(), failed);
    assert!(fork.pending_history().is_empty());
}

#[tokio::test]
async fn typed_content_and_restored_history_survive_without_text_conversion() {
    let (agent, mut prompts, _, _) = setup();
    let image: proto::ContentBlock = serde_json::from_value(serde_json::json!({
        "type": "image", "data": "aGVsbG8=", "mimeType": "image/png"
    }))
    .unwrap();
    let saved = vec![HistoryEntry {
        role: HistoryRole::User,
        content: vec![image.clone()],
    }];
    let fork = agent
        .fork_session_from_history(proto::NewSessionRequest::new("/tmp"), &saved, 1)
        .await
        .unwrap();
    let persisted = serde_json::to_string(fork.pending_history()).unwrap();
    let mut restored = cydonia_acp::client::HistoryFork::restore(
        agent,
        fork.session().clone(),
        serde_json::from_str(&persisted).unwrap(),
    );
    restored
        .prompt(proto::PromptRequest::new(
            "fork",
            vec!["describe it".into()],
        ))
        .await
        .unwrap();
    assert!(prompts.recv().await.unwrap().prompt.contains(&image));
}
