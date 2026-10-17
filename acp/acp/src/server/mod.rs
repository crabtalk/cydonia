//! Serve the agent side of ACP.
//!
//! Implement [`Agent`] and hand it to one of the `serve_*` functions. You get
//! back a [`ClientConn`] for calling the client — reporting progress, asking
//! permission, reading files through the editor.
//!
//! Every optional method of [`Agent`] defaults to replying "method not found",
//! so a client asking for something you do not serve gets a clean refusal
//! instead of waiting forever.

use crate::{Peer, codec::Tap};
use serve::Serve;
use std::sync::Arc;
use tokio::io::{AsyncBufRead, AsyncRead, AsyncWrite, BufReader};

pub use conn::ClientConn;
pub use role::Agent;

mod conn;
mod role;
mod serve;

/// Serve `agent` over a duplex stream — a Unix socket, an in-memory pipe.
pub fn serve<S, A>(stream: S, agent: Arc<A>, tap: Option<Tap>) -> ClientConn
where
    S: AsyncRead + AsyncWrite + Send + 'static,
    A: Agent,
{
    ClientConn(Peer::duplex(stream, Arc::new(Serve(agent)), tap))
}

/// Serve `agent` over a reader and a writer owned separately.
pub fn serve_on<R, W, A>(reader: R, writer: W, agent: Arc<A>, tap: Option<Tap>) -> ClientConn
where
    R: AsyncBufRead + Unpin + Send + 'static,
    W: AsyncWrite + Unpin + Send + 'static,
    A: Agent,
{
    ClientConn(Peer::new(reader, writer, Arc::new(Serve(agent)), tap))
}

/// Serve `agent` on this process's stdio, where an editor expects to find it.
pub fn serve_on_stdio<A: Agent>(agent: Arc<A>, tap: Option<Tap>) -> ClientConn {
    serve_on(
        BufReader::new(tokio::io::stdin()),
        tokio::io::stdout(),
        agent,
        tap,
    )
}
