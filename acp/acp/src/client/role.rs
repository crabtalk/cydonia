//! What an ACP client implements.

use crate::handler::{decode, encode};
use acp_proto::method::client as method;
use std::future::Future;

/// What an ACP client serves.
///
/// Only [`session_update`](Client::session_update) and
/// [`request_permission`](Client::request_permission) are required. `fs/*`,
/// `terminal/*` and `elicitation/*` default to replying "method not found", so
/// an agent that asks for something you do not serve gets a clean refusal and
/// carries on — declare what you do serve in
/// [`ClientCapabilities`](acp_proto::ClientCapabilities).
pub trait Client: Send + Sync + 'static {
    /// Progress for a turn: message chunks, tool calls, plans, mode changes.
    /// Awaited in wire order, so keep it to a channel send.
    fn session_update(
        &self,
        notification: acp_proto::SessionNotification,
    ) -> impl Future<Output = ()> + Send;

    /// Ask the user to authorize a tool call. Answering may take as long as the
    /// user does — it will not stall the update stream.
    fn request_permission(
        &self,
        request: acp_proto::RequestPermissionRequest,
    ) -> impl Future<Output = acp_proto::Result<acp_proto::RequestPermissionResponse>> + Send;

    fn read_text_file(
        &self,
        request: acp_proto::ReadTextFileRequest,
    ) -> impl Future<Output = acp_proto::Result<acp_proto::ReadTextFileResponse>> + Send {
        let _ = request;
        async { Err(acp_proto::Error::method_not_found()) }
    }

    fn write_text_file(
        &self,
        request: acp_proto::WriteTextFileRequest,
    ) -> impl Future<Output = acp_proto::Result<acp_proto::WriteTextFileResponse>> + Send {
        let _ = request;
        async { Err(acp_proto::Error::method_not_found()) }
    }

    fn create_terminal(
        &self,
        request: acp_proto::CreateTerminalRequest,
    ) -> impl Future<Output = acp_proto::Result<acp_proto::CreateTerminalResponse>> + Send {
        let _ = request;
        async { Err(acp_proto::Error::method_not_found()) }
    }

    fn terminal_output(
        &self,
        request: acp_proto::TerminalOutputRequest,
    ) -> impl Future<Output = acp_proto::Result<acp_proto::TerminalOutputResponse>> + Send {
        let _ = request;
        async { Err(acp_proto::Error::method_not_found()) }
    }

    fn release_terminal(
        &self,
        request: acp_proto::ReleaseTerminalRequest,
    ) -> impl Future<Output = acp_proto::Result<acp_proto::ReleaseTerminalResponse>> + Send {
        let _ = request;
        async { Err(acp_proto::Error::method_not_found()) }
    }

    fn wait_for_terminal_exit(
        &self,
        request: acp_proto::WaitForTerminalExitRequest,
    ) -> impl Future<Output = acp_proto::Result<acp_proto::WaitForTerminalExitResponse>> + Send
    {
        let _ = request;
        async { Err(acp_proto::Error::method_not_found()) }
    }

    fn kill_terminal(
        &self,
        request: acp_proto::KillTerminalRequest,
    ) -> impl Future<Output = acp_proto::Result<acp_proto::KillTerminalResponse>> + Send {
        let _ = request;
        async { Err(acp_proto::Error::method_not_found()) }
    }

    fn create_elicitation(
        &self,
        request: acp_proto::CreateElicitationRequest,
    ) -> impl Future<Output = acp_proto::Result<acp_proto::CreateElicitationResponse>> + Send {
        let _ = request;
        async { Err(acp_proto::Error::method_not_found()) }
    }

    /// A URL-mode elicitation finished out of band.
    fn complete_elicitation(
        &self,
        notification: acp_proto::CompleteElicitationNotification,
    ) -> impl Future<Output = ()> + Send {
        let _ = notification;
        async {}
    }

    /// Open an MCP connection on the agent's behalf. Serve this and the agent
    /// can use servers it could never reach itself — ones behind the editor's
    /// own credentials or process.
    fn connect_mcp(
        &self,
        request: acp_proto::ConnectMcpRequest,
    ) -> impl Future<Output = acp_proto::Result<acp_proto::ConnectMcpResponse>> + Send {
        let _ = request;
        async { Err(acp_proto::Error::method_not_found()) }
    }

    /// Forward one MCP call down a connection and hand back what the server
    /// said, unread.
    fn message_mcp(
        &self,
        request: acp_proto::MessageMcpRequest,
    ) -> impl Future<Output = acp_proto::Result<acp_proto::MessageMcpResponse>> + Send {
        let _ = request;
        async { Err(acp_proto::Error::method_not_found()) }
    }

    /// The same tunnel as [`message_mcp`](Client::message_mcp), for MCP
    /// messages that want no reply.
    fn notify_mcp(
        &self,
        notification: acp_proto::MessageMcpNotification,
    ) -> impl Future<Output = ()> + Send {
        let _ = notification;
        async {}
    }

    fn disconnect_mcp(
        &self,
        request: acp_proto::DisconnectMcpRequest,
    ) -> impl Future<Output = acp_proto::Result<acp_proto::DisconnectMcpResponse>> + Send {
        let _ = request;
        async { Err(acp_proto::Error::method_not_found()) }
    }

    /// Answer a method the spec does not define — anything the agent sends
    /// with a `_`-prefixed name. Declines by default.
    fn ext_request(
        &self,
        method: String,
        params: serde_json::Value,
    ) -> impl Future<Output = acp_proto::Result<serde_json::Value>> + Send {
        let _ = params;
        async move { Err(acp_proto::Error::method_not_found().data(method)) }
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
    ) -> impl Future<Output = acp_proto::Result<serde_json::Value>> + Send {
        async move {
            match method.as_str() {
                method::SESSION_REQUEST_PERMISSION => {
                    encode(self.request_permission(decode(params)?).await?)
                }
                method::FS_READ_TEXT_FILE => encode(self.read_text_file(decode(params)?).await?),
                method::FS_WRITE_TEXT_FILE => encode(self.write_text_file(decode(params)?).await?),
                method::TERMINAL_CREATE => encode(self.create_terminal(decode(params)?).await?),
                method::TERMINAL_OUTPUT => encode(self.terminal_output(decode(params)?).await?),
                method::TERMINAL_RELEASE => encode(self.release_terminal(decode(params)?).await?),
                method::TERMINAL_WAIT_FOR_EXIT => {
                    encode(self.wait_for_terminal_exit(decode(params)?).await?)
                }
                method::TERMINAL_KILL => encode(self.kill_terminal(decode(params)?).await?),
                method::ELICITATION_CREATE => {
                    encode(self.create_elicitation(decode(params)?).await?)
                }
                method::MCP_CONNECT => encode(self.connect_mcp(decode(params)?).await?),
                method::MCP_MESSAGE => encode(self.message_mcp(decode(params)?).await?),
                method::MCP_DISCONNECT => encode(self.disconnect_mcp(decode(params)?).await?),
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
                method::SESSION_UPDATE => {
                    if let Ok(notification) = decode(params) {
                        self.session_update(notification).await;
                    }
                }
                method::ELICITATION_COMPLETE => {
                    if let Ok(notification) = decode(params) {
                        self.complete_elicitation(notification).await;
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
