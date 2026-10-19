//! Cupola: a client and a server exchanging envelopes over one byte stream.
//!
//! The core carries [`wire::Envelope`]s: an id, the highest id received from
//! the peer, a service name and an opaque payload. What a payload means is
//! the business of a [`Service`], which names its own request, response and
//! event types. A [`Server`] and a [`Client`] alike register a handler per
//! service they answer, hand out a typed [`Handle`] per service the other
//! side answers, and push events through an [`Emitter`]. Cupola's own
//! services, which a server answers, are [`fs::Fs`] and [`exec::Exec`].
//!
//! A [`Connection`] is one pair of byte streams. Either side keeps every
//! envelope the peer has not acknowledged, and sends them again, in order,
//! when a new connection is attached. An envelope whose id is not above the
//! highest one received is dropped on arrival. Both sides outlive any one
//! connection.
//!
//! On Unix, a server runs as a [`daemon::Daemon`], reached through a
//! [`proxy`] on its stdio, which [`client::ssh::Remote`] starts over the
//! system `ssh`. A [`client::session::Session`] keeps a client attached: it
//! sends [`ping::Ping`] as a heartbeat and opens the transport again when the
//! connection ends or falls silent. A [`client::controller::Controller`]
//! keeps the saved hosts and one session per connected host.
//!
//! Nothing here spawns onto an executor: [`Server::attach`],
//! [`Client::attach`], [`client::session::Session::run`],
//! [`daemon::Daemon::run`] and the [`Runner`] of a server, a client or a
//! controller are futures the embedder polls on its own executor.

pub mod bytes;
pub mod client;
mod connection;
#[cfg(unix)]
pub mod daemon;
mod endpoint;
mod error;
pub mod exec;
pub mod fs;
pub mod lanes;
pub mod mock;
mod peer;
pub mod ping;
#[cfg(unix)]
pub mod proxy;
mod runner;
mod server;
mod service;
pub mod shell;
pub mod wire;

pub use client::Client;
pub use connection::Connection;
pub use endpoint::{Emitter, Events, Handle};
pub use error::Error;
pub use runner::{Runner, Spawner};
pub use server::Server;
pub use service::Service;
