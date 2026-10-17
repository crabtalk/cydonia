//! Running an agent as a subprocess and talking to it over its stdio.

use crate::{
    Peer,
    client::{AgentConn, Client, serve::Serve},
    codec::Tap,
};
use std::sync::Arc;
use tokio::{
    io::BufReader,
    process::{Child, Command},
};

/// Spawn `command` as an ACP agent and connect `client` to it.
///
/// Only stdin and stdout are taken; stderr is left as the caller configured it,
/// since a TUI usually wants it captured and a CLI usually does not. The agent
/// is killed when the returned [`Child`] is dropped.
///
/// That reaches the process spawned and no further. On Windows an agent
/// started through a `.cmd` launcher is a `cmd.exe` holding the real process,
/// which outlives it: put the child in a job object to end the whole tree.
pub fn spawn<C: Client>(
    command: &mut Command,
    client: Arc<C>,
    tap: Option<Tap>,
) -> acp_proto::Result<(AgentConn, Child)> {
    // Without this the agent outlives its `Child`: dropping the handle does not
    // signal it, and its stdin belongs to the write loop rather than to us.
    let mut child = command
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .kill_on_drop(true)
        .spawn()?;

    let stdin = child
        .stdin
        .take()
        .ok_or_else(|| acp_proto::Error::internal_error().data("agent stdin was not piped"))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| acp_proto::Error::internal_error().data("agent stdout was not piped"))?;

    let peer = Peer::new(BufReader::new(stdout), stdin, Arc::new(Serve(client)), tap);
    Ok((AgentConn(peer), child))
}
