//! A client's handle on the connected agent.

use crate::Peer;
use acp_proto::method::agent as method;

/// A client's handle on the connected agent.
#[derive(Clone)]
pub struct AgentConn(pub Peer);

impl AgentConn {
    /// Negotiate the protocol. The agent answers with the version it picked,
    /// which this build refuses unless it is one it speaks — proceeding on a
    /// version we cannot decode only fails later, and less clearly.
    pub async fn initialize(
        &self,
        request: acp_proto::InitializeRequest,
    ) -> acp_proto::Result<acp_proto::InitializeResponse> {
        let response: acp_proto::InitializeResponse =
            self.0.request(method::INITIALIZE, request).await?;
        if response.protocol_version != acp_proto::ProtocolVersion::LATEST {
            return Err(acp_proto::Error::invalid_request().data(format!(
                "agent chose protocol version {}; this build speaks {}",
                response.protocol_version.0,
                acp_proto::ProtocolVersion::LATEST.0
            )));
        }
        Ok(response)
    }

    pub async fn authenticate(
        &self,
        request: acp_proto::AuthenticateRequest,
    ) -> acp_proto::Result<acp_proto::AuthenticateResponse> {
        self.0.request(method::AUTHENTICATE, request).await
    }

    pub async fn logout(&self) -> acp_proto::Result<acp_proto::LogoutResponse> {
        self.0
            .request(method::LOGOUT, acp_proto::LogoutRequest::default())
            .await
    }

    pub async fn new_session(
        &self,
        request: acp_proto::NewSessionRequest,
    ) -> acp_proto::Result<acp_proto::NewSessionResponse> {
        self.0.request(method::SESSION_NEW, request).await
    }

    pub async fn load_session(
        &self,
        request: acp_proto::LoadSessionRequest,
    ) -> acp_proto::Result<acp_proto::LoadSessionResponse> {
        self.0.request(method::SESSION_LOAD, request).await
    }

    pub async fn resume_session(
        &self,
        request: acp_proto::ResumeSessionRequest,
    ) -> acp_proto::Result<acp_proto::ResumeSessionResponse> {
        self.0.request(method::SESSION_RESUME, request).await
    }

    /// Branch a session. The response carries the fork's id, not the
    /// original's.
    pub async fn fork_session(
        &self,
        request: acp_proto::ForkSessionRequest,
    ) -> acp_proto::Result<acp_proto::ForkSessionResponse> {
        self.0.request(method::SESSION_FORK, request).await
    }

    pub async fn close_session(
        &self,
        request: acp_proto::CloseSessionRequest,
    ) -> acp_proto::Result<acp_proto::CloseSessionResponse> {
        self.0.request(method::SESSION_CLOSE, request).await
    }

    pub async fn list_sessions(
        &self,
        request: acp_proto::ListSessionsRequest,
    ) -> acp_proto::Result<acp_proto::ListSessionsResponse> {
        self.0.request(method::SESSION_LIST, request).await
    }

    pub async fn delete_session(
        &self,
        request: acp_proto::DeleteSessionRequest,
    ) -> acp_proto::Result<acp_proto::DeleteSessionResponse> {
        self.0.request(method::SESSION_DELETE, request).await
    }

    pub async fn set_session_mode(
        &self,
        request: acp_proto::SetSessionModeRequest,
    ) -> acp_proto::Result<acp_proto::SetSessionModeResponse> {
        self.0.request(method::SESSION_SET_MODE, request).await
    }

    pub async fn set_session_config_option(
        &self,
        request: acp_proto::SetSessionConfigOptionRequest,
    ) -> acp_proto::Result<acp_proto::SetSessionConfigOptionResponse> {
        self.0
            .request(method::SESSION_SET_CONFIG_OPTION, request)
            .await
    }

    pub async fn prompt(
        &self,
        request: acp_proto::PromptRequest,
    ) -> acp_proto::Result<acp_proto::PromptResponse> {
        self.0.request(method::SESSION_PROMPT, request).await
    }

    /// Cancel the session's turn. The pending `prompt` still resolves, with
    /// [`StopReason::Cancelled`](acp_proto::StopReason::Cancelled).
    pub fn cancel(&self, notification: acp_proto::CancelNotification) -> acp_proto::Result<()> {
        self.0.notify(method::SESSION_CANCEL, notification)
    }

    pub async fn list_providers(
        &self,
        request: acp_proto::ListProvidersRequest,
    ) -> acp_proto::Result<acp_proto::ListProvidersResponse> {
        self.0.request(method::PROVIDERS_LIST, request).await
    }

    pub async fn set_provider(
        &self,
        request: acp_proto::SetProviderRequest,
    ) -> acp_proto::Result<acp_proto::SetProviderResponse> {
        self.0.request(method::PROVIDERS_SET, request).await
    }

    pub async fn disable_provider(
        &self,
        request: acp_proto::DisableProviderRequest,
    ) -> acp_proto::Result<acp_proto::DisableProviderResponse> {
        self.0.request(method::PROVIDERS_DISABLE, request).await
    }

    /// Open a next-edit session. Its id is unrelated to any chat session's.
    pub async fn start_nes(
        &self,
        request: acp_proto::StartNesRequest,
    ) -> acp_proto::Result<acp_proto::StartNesResponse> {
        self.0.request(method::NES_START, request).await
    }

    pub async fn suggest_nes(
        &self,
        request: acp_proto::SuggestNesRequest,
    ) -> acp_proto::Result<acp_proto::SuggestNesResponse> {
        self.0.request(method::NES_SUGGEST, request).await
    }

    pub async fn close_nes(
        &self,
        request: acp_proto::CloseNesRequest,
    ) -> acp_proto::Result<acp_proto::CloseNesResponse> {
        self.0.request(method::NES_CLOSE, request).await
    }

    pub fn accept_nes(
        &self,
        notification: acp_proto::AcceptNesNotification,
    ) -> acp_proto::Result<()> {
        self.0.notify(method::NES_ACCEPT, notification)
    }

    pub fn reject_nes(
        &self,
        notification: acp_proto::RejectNesNotification,
    ) -> acp_proto::Result<()> {
        self.0.notify(method::NES_REJECT, notification)
    }

    pub fn did_open_document(
        &self,
        notification: acp_proto::DidOpenDocumentNotification,
    ) -> acp_proto::Result<()> {
        self.0.notify(method::DOCUMENT_DID_OPEN, notification)
    }

    pub fn did_change_document(
        &self,
        notification: acp_proto::DidChangeDocumentNotification,
    ) -> acp_proto::Result<()> {
        self.0.notify(method::DOCUMENT_DID_CHANGE, notification)
    }

    pub fn did_close_document(
        &self,
        notification: acp_proto::DidCloseDocumentNotification,
    ) -> acp_proto::Result<()> {
        self.0.notify(method::DOCUMENT_DID_CLOSE, notification)
    }

    pub fn did_save_document(
        &self,
        notification: acp_proto::DidSaveDocumentNotification,
    ) -> acp_proto::Result<()> {
        self.0.notify(method::DOCUMENT_DID_SAVE, notification)
    }

    pub fn did_focus_document(
        &self,
        notification: acp_proto::DidFocusDocumentNotification,
    ) -> acp_proto::Result<()> {
        self.0.notify(method::DOCUMENT_DID_FOCUS, notification)
    }

    /// Relay an MCP message the client's own server sent, for the agent to
    /// answer.
    pub async fn message_mcp(
        &self,
        request: acp_proto::MessageMcpRequest,
    ) -> acp_proto::Result<acp_proto::MessageMcpResponse> {
        self.0.request(method::MCP_MESSAGE, request).await
    }

    pub fn notify_mcp(
        &self,
        notification: acp_proto::MessageMcpNotification,
    ) -> acp_proto::Result<()> {
        self.0.notify(method::MCP_MESSAGE, notification)
    }

    /// Call a method the spec does not define. Names must start with `_`.
    pub async fn ext_request(
        &self,
        method: &str,
        params: serde_json::Value,
    ) -> acp_proto::Result<serde_json::Value> {
        self.0.request(method, params).await
    }

    pub fn ext_notification(
        &self,
        method: &str,
        params: serde_json::Value,
    ) -> acp_proto::Result<()> {
        self.0.notify(method, params)
    }
}
