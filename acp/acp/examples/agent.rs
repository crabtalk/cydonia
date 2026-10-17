use cydonia_acp::{Agent, Result, schema};
use std::sync::Arc;

struct Echo;

impl Agent for Echo {
    async fn initialize(&self, _: schema::InitializeRequest) -> Result<schema::InitializeResponse> {
        Ok(schema::InitializeResponse {
            protocol_version: schema::ProtocolVersion::LATEST,
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
            session_id: "session-1".into(),
            modes: None,
            config_options: None,
            meta: None,
        })
    }

    async fn prompt(&self, _: schema::PromptRequest) -> Result<schema::PromptResponse> {
        Ok(schema::PromptResponse::new(schema::StopReason::EndTurn))
    }
}

#[tokio::main]
async fn main() {
    let _client = cydonia_acp::serve_on_stdio(Arc::new(Echo), None);
    // The read and write loops run on the runtime; keep the process alive.
    std::future::pending::<()>().await
}
