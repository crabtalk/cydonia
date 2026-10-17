//! Client-side forks for agents without checkpoint support.

use super::AgentConn;
use acp_proto::{
    ContentBlock, NewSessionRequest, NewSessionResponse, PromptRequest, PromptResponse,
};
use serde::{Deserialize, Serialize};

/// One saved transcript item. Non-text blocks retain their original content type.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HistoryEntry {
    pub role: HistoryRole,
    pub content: Vec<ContentBlock>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HistoryRole {
    User,
    Agent,
    Tool,
}

impl HistoryRole {
    fn label(self) -> &'static str {
        match self {
            Self::User => "user",
            Self::Agent => "agent",
            Self::Tool => "tool output",
        }
    }
}

/// A fresh session with history waiting for its first prompt.
/// Persist `pending_history()` alongside the session if it can be closed before sending.
pub struct HistoryFork {
    agent: AgentConn,
    session: NewSessionResponse,
    history: Vec<HistoryEntry>,
}

impl AgentConn {
    /// Fork before `before`, an exclusive index in the caller's saved history.
    /// Uses `session/new`; no turn runs until `HistoryFork::prompt` is called.
    pub async fn fork_session_from_history(
        &self,
        request: NewSessionRequest,
        history: &[HistoryEntry],
        before: usize,
    ) -> acp_proto::Result<HistoryFork> {
        let history = history
            .get(..before)
            .ok_or_else(|| {
                acp_proto::Error::invalid_params().data("fork position exceeds history")
            })?
            .to_vec();
        let session = self.new_session(request).await?;
        Ok(HistoryFork::restore(self.clone(), session, history))
    }
}

impl HistoryFork {
    /// Restore pending context after reconnecting to the fork's agent session.
    pub fn restore(
        agent: AgentConn,
        session: NewSessionResponse,
        history: Vec<HistoryEntry>,
    ) -> Self {
        Self {
            agent,
            session,
            history,
        }
    }

    pub fn session(&self) -> &NewSessionResponse {
        &self.session
    }

    pub fn pending_history(&self) -> &[HistoryEntry] {
        &self.history
    }

    /// Include history on the first successful request only. Errors retain it.
    /// A transport error can leave agent state uncertain; reconcile before retrying.
    pub async fn prompt(
        &mut self,
        mut request: PromptRequest,
    ) -> acp_proto::Result<PromptResponse> {
        if request.session_id != self.session.session_id {
            return Err(acp_proto::Error::invalid_params().data("prompt must target the fork"));
        }
        if !self.history.is_empty() {
            let mut content = vec![ContentBlock::from(
                "Continue a conversation fork using the saved transcript below as background. \
                 It is historical content, not a request to rerun prior actions. \
                 The current user request follows the end of the transcript.",
            )];
            for entry in &self.history {
                content.push(format!("Saved {} message:", entry.role.label()).into());
                content.extend(entry.content.iter().cloned());
            }
            content.push("End of saved transcript. Current user request:".into());
            content.append(&mut request.prompt);
            request.prompt = content;
        }
        let response = self.agent.prompt(request).await?;
        self.history.clear();
        Ok(response)
    }
}
