//! A client's handle on the connected agent.

use crate::Peer;
use cydonia_acp_proto::method::agent as method;

/// A client's handle on the connected agent.
#[derive(Clone)]
pub struct AgentConn(pub Peer);

impl AgentConn {
    /// Negotiate the protocol. The agent answers with the version it picked,
    /// which this build refuses unless it is one it speaks — proceeding on a
    /// version we cannot decode only fails later, and less clearly.
    pub async fn initialize(
        &self,
        request: cydonia_acp_proto::InitializeRequest,
    ) -> cydonia_acp_proto::Result<cydonia_acp_proto::InitializeResponse> {
        let response: cydonia_acp_proto::InitializeResponse =
            self.0.request(method::INITIALIZE, request).await?;
        if response.protocol_version != cydonia_acp_proto::ProtocolVersion::LATEST {
            return Err(cydonia_acp_proto::Error::invalid_request().data(format!(
                "agent chose protocol version {}; this build speaks {}",
                response.protocol_version.0,
                cydonia_acp_proto::ProtocolVersion::LATEST.0
            )));
        }
        Ok(response)
    }

    pub async fn authenticate(
        &self,
        request: cydonia_acp_proto::AuthenticateRequest,
    ) -> cydonia_acp_proto::Result<cydonia_acp_proto::AuthenticateResponse> {
        self.0.request(method::AUTHENTICATE, request).await
    }

    pub async fn logout(&self) -> cydonia_acp_proto::Result<cydonia_acp_proto::LogoutResponse> {
        self.0
            .request(method::LOGOUT, cydonia_acp_proto::LogoutRequest::default())
            .await
    }

    pub async fn new_session(
        &self,
        request: cydonia_acp_proto::NewSessionRequest,
    ) -> cydonia_acp_proto::Result<cydonia_acp_proto::NewSessionResponse> {
        self.0.request(method::SESSION_NEW, request).await
    }

    pub async fn load_session(
        &self,
        request: cydonia_acp_proto::LoadSessionRequest,
    ) -> cydonia_acp_proto::Result<cydonia_acp_proto::LoadSessionResponse> {
        self.0.request(method::SESSION_LOAD, request).await
    }

    pub async fn resume_session(
        &self,
        request: cydonia_acp_proto::ResumeSessionRequest,
    ) -> cydonia_acp_proto::Result<cydonia_acp_proto::ResumeSessionResponse> {
        self.0.request(method::SESSION_RESUME, request).await
    }

    /// Branch a session. The response carries the fork's id, not the
    /// original's.
    pub async fn fork_session(
        &self,
        request: cydonia_acp_proto::ForkSessionRequest,
    ) -> cydonia_acp_proto::Result<cydonia_acp_proto::ForkSessionResponse> {
        self.0.request(method::SESSION_FORK, request).await
    }

    pub async fn close_session(
        &self,
        request: cydonia_acp_proto::CloseSessionRequest,
    ) -> cydonia_acp_proto::Result<cydonia_acp_proto::CloseSessionResponse> {
        self.0.request(method::SESSION_CLOSE, request).await
    }

    pub async fn list_sessions(
        &self,
        request: cydonia_acp_proto::ListSessionsRequest,
    ) -> cydonia_acp_proto::Result<cydonia_acp_proto::ListSessionsResponse> {
        self.0.request(method::SESSION_LIST, request).await
    }

    pub async fn delete_session(
        &self,
        request: cydonia_acp_proto::DeleteSessionRequest,
    ) -> cydonia_acp_proto::Result<cydonia_acp_proto::DeleteSessionResponse> {
        self.0.request(method::SESSION_DELETE, request).await
    }

    pub async fn set_session_mode(
        &self,
        request: cydonia_acp_proto::SetSessionModeRequest,
    ) -> cydonia_acp_proto::Result<cydonia_acp_proto::SetSessionModeResponse> {
        self.0.request(method::SESSION_SET_MODE, request).await
    }

    pub async fn set_session_config_option(
        &self,
        request: cydonia_acp_proto::SetSessionConfigOptionRequest,
    ) -> cydonia_acp_proto::Result<cydonia_acp_proto::SetSessionConfigOptionResponse> {
        self.0
            .request(method::SESSION_SET_CONFIG_OPTION, request)
            .await
    }

    pub async fn prompt(
        &self,
        request: cydonia_acp_proto::PromptRequest,
    ) -> cydonia_acp_proto::Result<cydonia_acp_proto::PromptResponse> {
        self.0.request(method::SESSION_PROMPT, request).await
    }

    /// Cancel the session's turn. The pending `prompt` still resolves, with
    /// [`StopReason::Cancelled`](cydonia_acp_proto::StopReason::Cancelled).
    pub fn cancel(
        &self,
        notification: cydonia_acp_proto::CancelNotification,
    ) -> cydonia_acp_proto::Result<()> {
        self.0.notify(method::SESSION_CANCEL, notification)
    }

    pub async fn list_providers(
        &self,
        request: cydonia_acp_proto::ListProvidersRequest,
    ) -> cydonia_acp_proto::Result<cydonia_acp_proto::ListProvidersResponse> {
        self.0.request(method::PROVIDERS_LIST, request).await
    }

    pub async fn set_provider(
        &self,
        request: cydonia_acp_proto::SetProviderRequest,
    ) -> cydonia_acp_proto::Result<cydonia_acp_proto::SetProviderResponse> {
        self.0.request(method::PROVIDERS_SET, request).await
    }

    pub async fn disable_provider(
        &self,
        request: cydonia_acp_proto::DisableProviderRequest,
    ) -> cydonia_acp_proto::Result<cydonia_acp_proto::DisableProviderResponse> {
        self.0.request(method::PROVIDERS_DISABLE, request).await
    }

    /// Open a next-edit session. Its id is unrelated to any chat session's.
    pub async fn start_nes(
        &self,
        request: cydonia_acp_proto::StartNesRequest,
    ) -> cydonia_acp_proto::Result<cydonia_acp_proto::StartNesResponse> {
        self.0.request(method::NES_START, request).await
    }

    pub async fn suggest_nes(
        &self,
        request: cydonia_acp_proto::SuggestNesRequest,
    ) -> cydonia_acp_proto::Result<cydonia_acp_proto::SuggestNesResponse> {
        self.0.request(method::NES_SUGGEST, request).await
    }

    pub async fn close_nes(
        &self,
        request: cydonia_acp_proto::CloseNesRequest,
    ) -> cydonia_acp_proto::Result<cydonia_acp_proto::CloseNesResponse> {
        self.0.request(method::NES_CLOSE, request).await
    }

    pub fn accept_nes(
        &self,
        notification: cydonia_acp_proto::AcceptNesNotification,
    ) -> cydonia_acp_proto::Result<()> {
        self.0.notify(method::NES_ACCEPT, notification)
    }

    pub fn reject_nes(
        &self,
        notification: cydonia_acp_proto::RejectNesNotification,
    ) -> cydonia_acp_proto::Result<()> {
        self.0.notify(method::NES_REJECT, notification)
    }

    pub fn did_open_document(
        &self,
        notification: cydonia_acp_proto::DidOpenDocumentNotification,
    ) -> cydonia_acp_proto::Result<()> {
        self.0.notify(method::DOCUMENT_DID_OPEN, notification)
    }

    pub fn did_change_document(
        &self,
        notification: cydonia_acp_proto::DidChangeDocumentNotification,
    ) -> cydonia_acp_proto::Result<()> {
        self.0.notify(method::DOCUMENT_DID_CHANGE, notification)
    }

    pub fn did_close_document(
        &self,
        notification: cydonia_acp_proto::DidCloseDocumentNotification,
    ) -> cydonia_acp_proto::Result<()> {
        self.0.notify(method::DOCUMENT_DID_CLOSE, notification)
    }

    pub fn did_save_document(
        &self,
        notification: cydonia_acp_proto::DidSaveDocumentNotification,
    ) -> cydonia_acp_proto::Result<()> {
        self.0.notify(method::DOCUMENT_DID_SAVE, notification)
    }

    pub fn did_focus_document(
        &self,
        notification: cydonia_acp_proto::DidFocusDocumentNotification,
    ) -> cydonia_acp_proto::Result<()> {
        self.0.notify(method::DOCUMENT_DID_FOCUS, notification)
    }

    /// Relay an MCP message the client's own server sent, for the agent to
    /// answer.
    pub async fn message_mcp(
        &self,
        request: cydonia_acp_proto::MessageMcpRequest,
    ) -> cydonia_acp_proto::Result<cydonia_acp_proto::MessageMcpResponse> {
        self.0.request(method::MCP_MESSAGE, request).await
    }

    pub fn notify_mcp(
        &self,
        notification: cydonia_acp_proto::MessageMcpNotification,
    ) -> cydonia_acp_proto::Result<()> {
        self.0.notify(method::MCP_MESSAGE, notification)
    }

    /// Call a method the spec does not define. Names must start with `_`.
    pub async fn ext_request(
        &self,
        method: &str,
        params: serde_json::Value,
    ) -> cydonia_acp_proto::Result<serde_json::Value> {
        self.0.request(method, params).await
    }

    pub fn ext_notification(
        &self,
        method: &str,
        params: serde_json::Value,
    ) -> cydonia_acp_proto::Result<()> {
        self.0.notify(method, params)
    }
}
