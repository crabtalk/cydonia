//! Every [`Client`] call, forwarded as an [`Event`].

use crate::{Channel, Event, Reply};
use acp::Client;
use tokio::sync::oneshot;

impl Client for Channel {
    async fn session_update(&self, notification: acp_proto::SessionNotification) {
        self.tell(Event::Update(notification));
    }

    async fn request_permission(
        &self,
        request: acp_proto::RequestPermissionRequest,
    ) -> acp_proto::Result<acp_proto::RequestPermissionResponse> {
        self.ask(|reply| Event::Permission(request, reply)).await
    }

    async fn read_text_file(
        &self,
        request: acp_proto::ReadTextFileRequest,
    ) -> acp_proto::Result<acp_proto::ReadTextFileResponse> {
        self.ask(|reply| Event::ReadTextFile(request, reply)).await
    }

    async fn write_text_file(
        &self,
        request: acp_proto::WriteTextFileRequest,
    ) -> acp_proto::Result<acp_proto::WriteTextFileResponse> {
        self.ask(|reply| Event::WriteTextFile(request, reply)).await
    }

    async fn create_terminal(
        &self,
        request: acp_proto::CreateTerminalRequest,
    ) -> acp_proto::Result<acp_proto::CreateTerminalResponse> {
        self.ask(|reply| Event::CreateTerminal(request, reply))
            .await
    }

    async fn terminal_output(
        &self,
        request: acp_proto::TerminalOutputRequest,
    ) -> acp_proto::Result<acp_proto::TerminalOutputResponse> {
        self.ask(|reply| Event::TerminalOutput(request, reply))
            .await
    }

    async fn release_terminal(
        &self,
        request: acp_proto::ReleaseTerminalRequest,
    ) -> acp_proto::Result<acp_proto::ReleaseTerminalResponse> {
        self.ask(|reply| Event::ReleaseTerminal(request, reply))
            .await
    }

    async fn wait_for_terminal_exit(
        &self,
        request: acp_proto::WaitForTerminalExitRequest,
    ) -> acp_proto::Result<acp_proto::WaitForTerminalExitResponse> {
        self.ask(|reply| Event::WaitForTerminalExit(request, reply))
            .await
    }

    async fn kill_terminal(
        &self,
        request: acp_proto::KillTerminalRequest,
    ) -> acp_proto::Result<acp_proto::KillTerminalResponse> {
        self.ask(|reply| Event::KillTerminal(request, reply)).await
    }

    async fn create_elicitation(
        &self,
        request: acp_proto::CreateElicitationRequest,
    ) -> acp_proto::Result<acp_proto::CreateElicitationResponse> {
        self.ask(|reply| Event::CreateElicitation(request, reply))
            .await
    }

    async fn complete_elicitation(&self, notification: acp_proto::CompleteElicitationNotification) {
        self.tell(Event::CompleteElicitation(notification));
    }

    async fn connect_mcp(
        &self,
        request: acp_proto::ConnectMcpRequest,
    ) -> acp_proto::Result<acp_proto::ConnectMcpResponse> {
        self.ask(|reply| Event::ConnectMcp(request, reply)).await
    }

    async fn message_mcp(
        &self,
        request: acp_proto::MessageMcpRequest,
    ) -> acp_proto::Result<acp_proto::MessageMcpResponse> {
        self.ask(|reply| Event::MessageMcp(request, reply)).await
    }

    async fn notify_mcp(&self, notification: acp_proto::MessageMcpNotification) {
        self.tell(Event::NotifyMcp(notification));
    }

    async fn disconnect_mcp(
        &self,
        request: acp_proto::DisconnectMcpRequest,
    ) -> acp_proto::Result<acp_proto::DisconnectMcpResponse> {
        self.ask(|reply| Event::DisconnectMcp(request, reply)).await
    }

    async fn ext_request(
        &self,
        method: String,
        params: serde_json::Value,
    ) -> acp_proto::Result<serde_json::Value> {
        // The one call whose refusal names the method, matching the default.
        let declined = acp_proto::Error::method_not_found().data(method.clone());
        let (tx, rx) = oneshot::channel();
        let event = Event::ExtRequest {
            method,
            params,
            reply: Reply(tx),
        };
        if self.0.send(event).is_err() {
            return Err(declined);
        }
        rx.await.unwrap_or(Err(declined))
    }

    async fn ext_notification(&self, method: String, params: serde_json::Value) {
        self.tell(Event::ExtNotification { method, params });
    }
}
