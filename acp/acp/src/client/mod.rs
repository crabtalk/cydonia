//! Drive an ACP agent from a client.
//!
//! Implement [`Client`] and hand it to [`spawn`] or one of the `connect_*`
//! functions. You get back an [`AgentConn`] for calling the agent — opening
//! sessions, sending prompts, cancelling turns.
//!
//! Every optional method of [`Client`] defaults to replying "method not found",
//! so an agent asking for something you do not serve gets a clean refusal and
//! carries on.
//!
//! A frontend with an event loop of its own can take `cydonia-acp-events` instead,
//! which serves this trait over a channel.

use crate::{Peer, codec::Tap};
use serve::Serve;
use std::sync::Arc;
use tokio::io::{AsyncBufRead, AsyncRead, AsyncWrite};

pub use conn::AgentConn;
pub use history::{HistoryEntry, HistoryFork, HistoryRole};
pub use role::Client;
pub use spawn::spawn;

mod conn;
mod history;
mod role;
mod serve;
mod spawn;

/// Connect `client` to an agent over a duplex stream. Use [`spawn`] instead
/// when the agent is a subprocess.
pub fn connect<S, C>(stream: S, client: Arc<C>, tap: Option<Tap>) -> AgentConn
where
    S: AsyncRead + AsyncWrite + Send + 'static,
    C: Client,
{
    AgentConn(Peer::duplex(stream, Arc::new(Serve(client)), tap))
}

/// Connect `client` to an agent over a reader and a writer owned separately.
pub fn connect_on<R, W, C>(reader: R, writer: W, client: Arc<C>, tap: Option<Tap>) -> AgentConn
where
    R: AsyncBufRead + Unpin + Send + 'static,
    W: AsyncWrite + Unpin + Send + 'static,
    C: Client,
{
    AgentConn(Peer::new(reader, writer, Arc::new(Serve(client)), tap))
}
