//! An agent's handle on the connected client.

use crate::Peer;
use cydonia_acp_proto::method::client as method;

/// An agent's handle on the connected client.
#[derive(Clone)]
pub struct ClientConn(pub Peer);

impl ClientConn {
    pub fn session_update(
        &self,
        notification: cydonia_acp_proto::SessionNotification,
    ) -> cydonia_acp_proto::Result<()> {
        self.0.notify(method::SESSION_UPDATE, notification)
    }

    pub async fn request_permission(
        &self,
        request: cydonia_acp_proto::RequestPermissionRequest,
    ) -> cydonia_acp_proto::Result<cydonia_acp_proto::RequestPermissionResponse> {
        self.0
            .request(method::SESSION_REQUEST_PERMISSION, request)
            .await
    }

    pub async fn read_text_file(
        &self,
        request: cydonia_acp_proto::ReadTextFileRequest,
    ) -> cydonia_acp_proto::Result<cydonia_acp_proto::ReadTextFileResponse> {
        self.0.request(method::FS_READ_TEXT_FILE, request).await
    }

    pub async fn write_text_file(
        &self,
        request: cydonia_acp_proto::WriteTextFileRequest,
    ) -> cydonia_acp_proto::Result<cydonia_acp_proto::WriteTextFileResponse> {
        self.0.request(method::FS_WRITE_TEXT_FILE, request).await
    }

    pub async fn create_terminal(
        &self,
        request: cydonia_acp_proto::CreateTerminalRequest,
    ) -> cydonia_acp_proto::Result<cydonia_acp_proto::CreateTerminalResponse> {
        self.0.request(method::TERMINAL_CREATE, request).await
    }

    pub async fn terminal_output(
        &self,
        request: cydonia_acp_proto::TerminalOutputRequest,
    ) -> cydonia_acp_proto::Result<cydonia_acp_proto::TerminalOutputResponse> {
        self.0.request(method::TERMINAL_OUTPUT, request).await
    }

    pub async fn release_terminal(
        &self,
        request: cydonia_acp_proto::ReleaseTerminalRequest,
    ) -> cydonia_acp_proto::Result<cydonia_acp_proto::ReleaseTerminalResponse> {
        self.0.request(method::TERMINAL_RELEASE, request).await
    }

    pub async fn wait_for_terminal_exit(
        &self,
        request: cydonia_acp_proto::WaitForTerminalExitRequest,
    ) -> cydonia_acp_proto::Result<cydonia_acp_proto::WaitForTerminalExitResponse> {
        self.0
            .request(method::TERMINAL_WAIT_FOR_EXIT, request)
            .await
    }

    pub async fn kill_terminal(
        &self,
        request: cydonia_acp_proto::KillTerminalRequest,
    ) -> cydonia_acp_proto::Result<cydonia_acp_proto::KillTerminalResponse> {
        self.0.request(method::TERMINAL_KILL, request).await
    }

    pub async fn create_elicitation(
        &self,
        request: cydonia_acp_proto::CreateElicitationRequest,
    ) -> cydonia_acp_proto::Result<cydonia_acp_proto::CreateElicitationResponse> {
        self.0.request(method::ELICITATION_CREATE, request).await
    }

    pub fn complete_elicitation(
        &self,
        notification: cydonia_acp_proto::CompleteElicitationNotification,
    ) -> cydonia_acp_proto::Result<()> {
        self.0.notify(method::ELICITATION_COMPLETE, notification)
    }

    /// Ask the client to open an MCP connection this agent cannot open
    /// itself.
    pub async fn connect_mcp(
        &self,
        request: cydonia_acp_proto::ConnectMcpRequest,
    ) -> cydonia_acp_proto::Result<cydonia_acp_proto::ConnectMcpResponse> {
        self.0.request(method::MCP_CONNECT, request).await
    }

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

    pub async fn disconnect_mcp(
        &self,
        request: cydonia_acp_proto::DisconnectMcpRequest,
    ) -> cydonia_acp_proto::Result<cydonia_acp_proto::DisconnectMcpResponse> {
        self.0.request(method::MCP_DISCONNECT, request).await
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
