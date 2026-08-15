//! ACP session core shared by cydonia frontends.
//!
//! Executor-neutral on purpose: the TUI drives it under tokio, a GPUI
//! frontend can drive it on its own executor. Keep tokio out of here.

pub use agent_client_protocol as acp;

pub mod session;
pub mod settings;
