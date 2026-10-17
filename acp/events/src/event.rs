//! Everything an agent asks of a client, as one value.

use crate::Reply;

/// One inbound call from the agent.
///
/// A variant carrying a [`Reply`] is a request: answer it, or drop the reply to
/// decline. The rest are notifications.
#[derive(Debug)]
#[non_exhaustive]
pub enum Event {
    Update(cydonia_acp_proto::SessionNotification),
    Permission(
        cydonia_acp_proto::RequestPermissionRequest,
        Reply<cydonia_acp_proto::RequestPermissionResponse>,
    ),
    ReadTextFile(
        cydonia_acp_proto::ReadTextFileRequest,
        Reply<cydonia_acp_proto::ReadTextFileResponse>,
    ),
    WriteTextFile(
        cydonia_acp_proto::WriteTextFileRequest,
        Reply<cydonia_acp_proto::WriteTextFileResponse>,
    ),
    CreateTerminal(
        cydonia_acp_proto::CreateTerminalRequest,
        Reply<cydonia_acp_proto::CreateTerminalResponse>,
    ),
    TerminalOutput(
        cydonia_acp_proto::TerminalOutputRequest,
        Reply<cydonia_acp_proto::TerminalOutputResponse>,
    ),
    ReleaseTerminal(
        cydonia_acp_proto::ReleaseTerminalRequest,
        Reply<cydonia_acp_proto::ReleaseTerminalResponse>,
    ),
    WaitForTerminalExit(
        cydonia_acp_proto::WaitForTerminalExitRequest,
        Reply<cydonia_acp_proto::WaitForTerminalExitResponse>,
    ),
    KillTerminal(
        cydonia_acp_proto::KillTerminalRequest,
        Reply<cydonia_acp_proto::KillTerminalResponse>,
    ),
    CreateElicitation(
        cydonia_acp_proto::CreateElicitationRequest,
        Reply<cydonia_acp_proto::CreateElicitationResponse>,
    ),
    CompleteElicitation(cydonia_acp_proto::CompleteElicitationNotification),
    ConnectMcp(
        cydonia_acp_proto::ConnectMcpRequest,
        Reply<cydonia_acp_proto::ConnectMcpResponse>,
    ),
    MessageMcp(
        cydonia_acp_proto::MessageMcpRequest,
        Reply<cydonia_acp_proto::MessageMcpResponse>,
    ),
    NotifyMcp(cydonia_acp_proto::MessageMcpNotification),
    DisconnectMcp(
        cydonia_acp_proto::DisconnectMcpRequest,
        Reply<cydonia_acp_proto::DisconnectMcpResponse>,
    ),
    ExtRequest {
        method: String,
        params: serde_json::Value,
        reply: Reply<serde_json::Value>,
    },
    ExtNotification {
        method: String,
        params: serde_json::Value,
    },
}
