//! Everything an agent asks of a client, as one value.

use crate::Reply;

/// One inbound call from the agent.
///
/// A variant carrying a [`Reply`] is a request: answer it, or drop the reply to
/// decline. The rest are notifications.
#[derive(Debug)]
#[non_exhaustive]
pub enum Event {
    Update(acp_proto::SessionNotification),
    Permission(
        acp_proto::RequestPermissionRequest,
        Reply<acp_proto::RequestPermissionResponse>,
    ),
    ReadTextFile(
        acp_proto::ReadTextFileRequest,
        Reply<acp_proto::ReadTextFileResponse>,
    ),
    WriteTextFile(
        acp_proto::WriteTextFileRequest,
        Reply<acp_proto::WriteTextFileResponse>,
    ),
    CreateTerminal(
        acp_proto::CreateTerminalRequest,
        Reply<acp_proto::CreateTerminalResponse>,
    ),
    TerminalOutput(
        acp_proto::TerminalOutputRequest,
        Reply<acp_proto::TerminalOutputResponse>,
    ),
    ReleaseTerminal(
        acp_proto::ReleaseTerminalRequest,
        Reply<acp_proto::ReleaseTerminalResponse>,
    ),
    WaitForTerminalExit(
        acp_proto::WaitForTerminalExitRequest,
        Reply<acp_proto::WaitForTerminalExitResponse>,
    ),
    KillTerminal(
        acp_proto::KillTerminalRequest,
        Reply<acp_proto::KillTerminalResponse>,
    ),
    CreateElicitation(
        acp_proto::CreateElicitationRequest,
        Reply<acp_proto::CreateElicitationResponse>,
    ),
    CompleteElicitation(acp_proto::CompleteElicitationNotification),
    ConnectMcp(
        acp_proto::ConnectMcpRequest,
        Reply<acp_proto::ConnectMcpResponse>,
    ),
    MessageMcp(
        acp_proto::MessageMcpRequest,
        Reply<acp_proto::MessageMcpResponse>,
    ),
    NotifyMcp(acp_proto::MessageMcpNotification),
    DisconnectMcp(
        acp_proto::DisconnectMcpRequest,
        Reply<acp_proto::DisconnectMcpResponse>,
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
