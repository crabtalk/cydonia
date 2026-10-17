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
//! The wire types are [`proto`]. The `agents` feature adds [`agents`], the ACP
//! agent registry, and `install` its installer. With `client`, `server` and
//! `peer` off, the crate pulls in no async runtime.
//!
//! [acp]: https://agentclientprotocol.com

#[cfg(feature = "agents")]
pub use cydonia_acp_agents as agents;
pub use cydonia_acp_proto::{self as proto, Error, RequestId, Result, method};
#[cfg(feature = "peer")]
pub use {
    codec::{Direction, Message, Tap},
    handler::Handler,
    peer::Peer,
};

#[cfg(feature = "peer")]
pub mod codec;

#[cfg(feature = "client")]
pub mod client;
#[cfg(feature = "server")]
pub mod server;

#[cfg(feature = "peer")]
mod handler;
#[cfg(feature = "peer")]
mod peer;

#[cfg(feature = "client")]
pub use client::{AgentConn, Client, connect, connect_on, spawn};

#[cfg(feature = "server")]
pub use server::{Agent, ClientConn, serve, serve_on, serve_on_stdio};
