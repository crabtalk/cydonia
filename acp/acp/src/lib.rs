//! A compact implementation of the [Agent Client Protocol][acp] v1.
//!
//! Two traits carry the protocol. An agent implements [`Agent`] and gets a
//! [`ClientConn`] to call the client with; a client implements [`Client`]
//! and gets an [`AgentConn`] to call the agent with. Routing is provided —
//! you write handlers, never a dispatch table.
//!
//! Every optional method defaults to replying "method not found", so a peer
//! that asks for something you do not serve gets a clean refusal rather than
//! waiting forever.
//!
//! Each side is a feature, and both are on by default. Turn off the one you
//! do not implement:
//!
//! ```toml
//! cydonia-acp = { version = "0.2.0", default-features = false, features = ["client"] }
//! ```
//!
//! Depend on `cydonia-acp-proto` instead if you only need the wire types; it pulls
//! in no async runtime.
//!
//! [acp]: https://agentclientprotocol.com

pub use acp_proto::{self as schema, Error, RequestId, Result, method};
pub use codec::{Direction, Message, Tap};
pub use handler::Handler;
pub use peer::Peer;

pub mod codec;

#[cfg(feature = "client")]
pub mod client;
#[cfg(feature = "server")]
pub mod server;

mod handler;
mod peer;

#[cfg(feature = "client")]
pub use client::{AgentConn, Client, connect, connect_on, spawn};

#[cfg(feature = "server")]
pub use server::{Agent, ClientConn, serve, serve_on, serve_on_stdio};
