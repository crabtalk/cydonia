use cydonia_acp::{Agent, Result, proto};
use std::sync::Arc;

struct Echo;

impl Agent for Echo {
    async fn initialize(&self, _: proto::InitializeRequest) -> Result<proto::InitializeResponse> {
        Ok(proto::InitializeResponse {
            protocol_version: proto::ProtocolVersion::LATEST,
            agent_capabilities: Default::default(),
            auth_methods: Vec::new(),
            agent_info: None,
            meta: None,
        })
    }

    async fn new_session(&self, _: proto::NewSessionRequest) -> Result<proto::NewSessionResponse> {
        Ok(proto::NewSessionResponse {
            session_id: "session-1".into(),
            modes: None,
            config_options: None,
            meta: None,
        })
    }

    async fn prompt(&self, _: proto::PromptRequest) -> Result<proto::PromptResponse> {
        Ok(proto::PromptResponse::new(proto::StopReason::EndTurn))
    }
}

#[tokio::main]
async fn main() {
    let _client = cydonia_acp::serve_on_stdio(Arc::new(Echo), None);
    // The read and write loops run on the runtime; keep the process alive.
    std::future::pending::<()>().await
}
