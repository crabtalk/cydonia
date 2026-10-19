//! The MCP server cydonia answers as: the tools an agent works a project
//! through.
//!
//! What carried a call is not a tool's business. [`Server::handle`] takes a
//! request that is already decoded and answers one, so the HTTP door hands it
//! a parsed body and the ACP tunnel hands it params that were never framed at
//! all — and neither of them is named in here.
//!
//! Nothing here holds a project. One server answers for the whole app, and
//! which project a call is about is an argument on the call or the binding it
//! arrived with, named by its [`artifact::location::Location`]. A tool reads
//! and writes it through the store [`rail::store`] answers for that location.

#[cfg(feature = "http")]
pub mod http;
pub mod proto;
pub mod rail;
#[cfg(feature = "http")]
mod resources;
#[cfg(feature = "http")]
mod server;
pub mod tool;
#[cfg(feature = "http")]
pub mod tools;

#[cfg(feature = "http")]
pub use server::*;
