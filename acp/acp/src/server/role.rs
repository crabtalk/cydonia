//! What an ACP agent implements.

use crate::handler::{decode, encode};
use cydonia_acp_proto::method::agent as method;
use std::future::Future;

/// What an ACP agent serves.
///
/// Only [`initialize`](Agent::initialize), [`new_session`](Agent::new_session)
/// and [`prompt`](Agent::prompt) are required. Every other method defaults to
/// replying "method not found", so an agent that does not implement a capability
/// declines it instead of leaving the client waiting — advertise what you do
/// implement in [`cydonia_acp_proto::InitializeResponse::agent_capabilities`].
pub trait Agent: Send + Sync + 'static {
    fn initialize(
        &self,
        request: cydonia_acp_proto::InitializeRequest,
    ) -> impl Future<Output = cydonia_acp_proto::Result<cydonia_acp_proto::InitializeResponse>> + Send;

    fn new_session(
        &self,
        request: cydonia_acp_proto::NewSessionRequest,
    ) -> impl Future<Output = cydonia_acp_proto::Result<cydonia_acp_proto::NewSessionResponse>> + Send;

    /// Run a turn. Report progress with
    /// [`ClientConn::session_update`](crate::ClientConn::session_update) as it goes.
    fn prompt(
        &self,
        request: cydonia_acp_proto::PromptRequest,
    ) -> impl Future<Output = cydonia_acp_proto::Result<cydonia_acp_proto::PromptResponse>> + Send;

    /// Stop the session's current turn. The in-flight `prompt` should still
    /// return, with [`StopReason::Cancelled`](cydonia_acp_proto::StopReason::Cancelled).
    fn cancel(
        &self,
        notification: cydonia_acp_proto::CancelNotification,
    ) -> impl Future<Output = ()> + Send {
        let _ = notification;
        async {}
    }

    fn authenticate(
        &self,
        request: cydonia_acp_proto::AuthenticateRequest,
    ) -> impl Future<Output = cydonia_acp_proto::Result<cydonia_acp_proto::AuthenticateResponse>> + Send
    {
        let _ = request;
        async { Err(cydonia_acp_proto::Error::method_not_found()) }
    }

    fn logout(
        &self,
        request: cydonia_acp_proto::LogoutRequest,
    ) -> impl Future<Output = cydonia_acp_proto::Result<cydonia_acp_proto::LogoutResponse>> + Send
    {
        let _ = request;
        async { Err(cydonia_acp_proto::Error::method_not_found()) }
    }

    fn load_session(
        &self,
        request: cydonia_acp_proto::LoadSessionRequest,
    ) -> impl Future<Output = cydonia_acp_proto::Result<cydonia_acp_proto::LoadSessionResponse>> + Send
    {
        let _ = request;
        async { Err(cydonia_acp_proto::Error::method_not_found()) }
    }

    fn resume_session(
        &self,
        request: cydonia_acp_proto::ResumeSessionRequest,
    ) -> impl Future<Output = cydonia_acp_proto::Result<cydonia_acp_proto::ResumeSessionResponse>> + Send
    {
        let _ = request;
        async { Err(cydonia_acp_proto::Error::method_not_found()) }
    }

    /// Branch a session. The response carries the *new* session's id; the
    /// one in the request keeps running untouched.
    fn fork_session(
        &self,
        request: cydonia_acp_proto::ForkSessionRequest,
    ) -> impl Future<Output = cydonia_acp_proto::Result<cydonia_acp_proto::ForkSessionResponse>> + Send
    {
        let _ = request;
        async { Err(cydonia_acp_proto::Error::method_not_found()) }
    }

    fn close_session(
        &self,
        request: cydonia_acp_proto::CloseSessionRequest,
    ) -> impl Future<Output = cydonia_acp_proto::Result<cydonia_acp_proto::CloseSessionResponse>> + Send
    {
        let _ = request;
        async { Err(cydonia_acp_proto::Error::method_not_found()) }
    }

    fn list_sessions(
        &self,
        request: cydonia_acp_proto::ListSessionsRequest,
    ) -> impl Future<Output = cydonia_acp_proto::Result<cydonia_acp_proto::ListSessionsResponse>> + Send
    {
        let _ = request;
        async { Err(cydonia_acp_proto::Error::method_not_found()) }
    }

    fn delete_session(
        &self,
        request: cydonia_acp_proto::DeleteSessionRequest,
    ) -> impl Future<Output = cydonia_acp_proto::Result<cydonia_acp_proto::DeleteSessionResponse>> + Send
    {
        let _ = request;
        async { Err(cydonia_acp_proto::Error::method_not_found()) }
    }

    fn set_session_mode(
        &self,
        request: cydonia_acp_proto::SetSessionModeRequest,
    ) -> impl Future<Output = cydonia_acp_proto::Result<cydonia_acp_proto::SetSessionModeResponse>> + Send
    {
        let _ = request;
        async { Err(cydonia_acp_proto::Error::method_not_found()) }
    }

    fn set_session_config_option(
        &self,
        request: cydonia_acp_proto::SetSessionConfigOptionRequest,
    ) -> impl Future<
        Output = cydonia_acp_proto::Result<cydonia_acp_proto::SetSessionConfigOptionResponse>,
    > + Send {
        let _ = request;
        async { Err(cydonia_acp_proto::Error::method_not_found()) }
    }

    /// The LLM endpoints this agent can be pointed at, and where each one
    /// points right now.
    fn list_providers(
        &self,
        request: cydonia_acp_proto::ListProvidersRequest,
    ) -> impl Future<Output = cydonia_acp_proto::Result<cydonia_acp_proto::ListProvidersResponse>> + Send
    {
        let _ = request;
        async { Err(cydonia_acp_proto::Error::method_not_found()) }
    }

    fn set_provider(
        &self,
        request: cydonia_acp_proto::SetProviderRequest,
    ) -> impl Future<Output = cydonia_acp_proto::Result<cydonia_acp_proto::SetProviderResponse>> + Send
    {
        let _ = request;
        async { Err(cydonia_acp_proto::Error::method_not_found()) }
    }

    fn disable_provider(
        &self,
        request: cydonia_acp_proto::DisableProviderRequest,
    ) -> impl Future<Output = cydonia_acp_proto::Result<cydonia_acp_proto::DisableProviderResponse>> + Send
    {
        let _ = request;
        async { Err(cydonia_acp_proto::Error::method_not_found()) }
    }

    /// Open a next-edit session. Its id is its own — it does not name a chat
    /// session, and the two lifecycles are unrelated.
    fn start_nes(
        &self,
        request: cydonia_acp_proto::StartNesRequest,
    ) -> impl Future<Output = cydonia_acp_proto::Result<cydonia_acp_proto::StartNesResponse>> + Send
    {
        let _ = request;
        async { Err(cydonia_acp_proto::Error::method_not_found()) }
    }

    /// Predict the user's next edit. Called on every keystroke a client
    /// decides is worth asking about, so it should be cheap to decline.
    fn suggest_nes(
        &self,
        request: cydonia_acp_proto::SuggestNesRequest,
    ) -> impl Future<Output = cydonia_acp_proto::Result<cydonia_acp_proto::SuggestNesResponse>> + Send
    {
        let _ = request;
        async { Err(cydonia_acp_proto::Error::method_not_found()) }
    }

    fn close_nes(
        &self,
        request: cydonia_acp_proto::CloseNesRequest,
    ) -> impl Future<Output = cydonia_acp_proto::Result<cydonia_acp_proto::CloseNesResponse>> + Send
    {
        let _ = request;
        async { Err(cydonia_acp_proto::Error::method_not_found()) }
    }

    /// The user took a suggestion.
    fn accept_nes(
        &self,
        notification: cydonia_acp_proto::AcceptNesNotification,
    ) -> impl Future<Output = ()> + Send {
        let _ = notification;
        async {}
    }

    /// The user did not take a suggestion — see
    /// [`NesRejectReason`](cydonia_acp_proto::NesRejectReason) for why.
    fn reject_nes(
        &self,
        notification: cydonia_acp_proto::RejectNesNotification,
    ) -> impl Future<Output = ()> + Send {
        let _ = notification;
        async {}
    }

    fn did_open_document(
        &self,
        notification: cydonia_acp_proto::DidOpenDocumentNotification,
    ) -> impl Future<Output = ()> + Send {
        let _ = notification;
        async {}
    }

    fn did_change_document(
        &self,
        notification: cydonia_acp_proto::DidChangeDocumentNotification,
    ) -> impl Future<Output = ()> + Send {
        let _ = notification;
        async {}
    }

    fn did_close_document(
        &self,
        notification: cydonia_acp_proto::DidCloseDocumentNotification,
    ) -> impl Future<Output = ()> + Send {
        let _ = notification;
        async {}
    }

    fn did_save_document(
        &self,
        notification: cydonia_acp_proto::DidSaveDocumentNotification,
    ) -> impl Future<Output = ()> + Send {
        let _ = notification;
        async {}
    }

    fn did_focus_document(
        &self,
        notification: cydonia_acp_proto::DidFocusDocumentNotification,
    ) -> impl Future<Output = ()> + Send {
        let _ = notification;
        async {}
    }

    /// An MCP message coming *back* up the tunnel — a server the client holds
    /// on this agent's behalf is asking the agent something.
    fn message_mcp(
        &self,
        request: cydonia_acp_proto::MessageMcpRequest,
    ) -> impl Future<Output = cydonia_acp_proto::Result<cydonia_acp_proto::MessageMcpResponse>> + Send
    {
        let _ = request;
        async { Err(cydonia_acp_proto::Error::method_not_found()) }
    }

    /// The same tunnel as [`message_mcp`](Agent::message_mcp), for MCP
    /// messages that want no reply.
    fn notify_mcp(
        &self,
        notification: cydonia_acp_proto::MessageMcpNotification,
    ) -> impl Future<Output = ()> + Send {
        let _ = notification;
        async {}
    }

    /// Answer a method the spec does not define — anything the client sends
    /// with a `_`-prefixed name. Declines by default.
    fn ext_request(
        &self,
        method: String,
        params: serde_json::Value,
    ) -> impl Future<Output = cydonia_acp_proto::Result<serde_json::Value>> + Send {
        let _ = params;
        async move { Err(cydonia_acp_proto::Error::method_not_found().data(method)) }
    }

    /// The same, for a notification. Ignored by default.
    fn ext_notification(
        &self,
        method: String,
        params: serde_json::Value,
    ) -> impl Future<Output = ()> + Send {
        let _ = (method, params);
        async {}
    }

    /// Route one incoming call. Provided — do not override.
    fn dispatch(
        &self,
        method: String,
        params: serde_json::Value,
    ) -> impl Future<Output = cydonia_acp_proto::Result<serde_json::Value>> + Send {
        async move {
            match method.as_str() {
                method::INITIALIZE => encode(self.initialize(decode(params)?).await?),
                method::AUTHENTICATE => encode(self.authenticate(decode(params)?).await?),
                method::LOGOUT => encode(self.logout(decode(params)?).await?),
                method::SESSION_NEW => encode(self.new_session(decode(params)?).await?),
                method::SESSION_LOAD => encode(self.load_session(decode(params)?).await?),
                method::SESSION_RESUME => encode(self.resume_session(decode(params)?).await?),
                method::SESSION_FORK => encode(self.fork_session(decode(params)?).await?),
                method::SESSION_CLOSE => encode(self.close_session(decode(params)?).await?),
                method::SESSION_LIST => encode(self.list_sessions(decode(params)?).await?),
                method::SESSION_DELETE => encode(self.delete_session(decode(params)?).await?),
                method::SESSION_SET_MODE => encode(self.set_session_mode(decode(params)?).await?),
                method::SESSION_SET_CONFIG_OPTION => {
                    encode(self.set_session_config_option(decode(params)?).await?)
                }
                method::SESSION_PROMPT => encode(self.prompt(decode(params)?).await?),
                method::PROVIDERS_LIST => encode(self.list_providers(decode(params)?).await?),
                method::PROVIDERS_SET => encode(self.set_provider(decode(params)?).await?),
                method::PROVIDERS_DISABLE => encode(self.disable_provider(decode(params)?).await?),
                method::NES_START => encode(self.start_nes(decode(params)?).await?),
                method::NES_SUGGEST => encode(self.suggest_nes(decode(params)?).await?),
                method::NES_CLOSE => encode(self.close_nes(decode(params)?).await?),
                method::MCP_MESSAGE => encode(self.message_mcp(decode(params)?).await?),
                _ => self.ext_request(method, params).await,
            }
        }
    }

    /// Route one incoming notification. Provided — do not override.
    fn dispatch_notification(
        &self,
        method: String,
        params: serde_json::Value,
    ) -> impl Future<Output = ()> + Send {
        async move {
            match method.as_str() {
                method::SESSION_CANCEL => {
                    if let Ok(notification) = decode(params) {
                        self.cancel(notification).await;
                    }
                }
                method::NES_ACCEPT => {
                    if let Ok(notification) = decode(params) {
                        self.accept_nes(notification).await;
                    }
                }
                method::NES_REJECT => {
                    if let Ok(notification) = decode(params) {
                        self.reject_nes(notification).await;
                    }
                }
                method::DOCUMENT_DID_OPEN => {
                    if let Ok(notification) = decode(params) {
                        self.did_open_document(notification).await;
                    }
                }
                method::DOCUMENT_DID_CHANGE => {
                    if let Ok(notification) = decode(params) {
                        self.did_change_document(notification).await;
                    }
                }
                method::DOCUMENT_DID_CLOSE => {
                    if let Ok(notification) = decode(params) {
                        self.did_close_document(notification).await;
                    }
                }
                method::DOCUMENT_DID_SAVE => {
                    if let Ok(notification) = decode(params) {
                        self.did_save_document(notification).await;
                    }
                }
                method::DOCUMENT_DID_FOCUS => {
                    if let Ok(notification) = decode(params) {
                        self.did_focus_document(notification).await;
                    }
                }
                method::MCP_MESSAGE => {
                    if let Ok(notification) = decode(params) {
                        self.notify_mcp(notification).await;
                    }
                }
                _ => self.ext_notification(method, params).await,
            }
        }
    }
}
