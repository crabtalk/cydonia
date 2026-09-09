//! What cydonia keeps: the boards, session records, article properties and
//! tables a project holds, as shapes rather than as files.
//!
//! The filesystem is one backend for these and not the definition of them. A
//! board is a board whether it was read out of `.cydonia/boards/` or handed
//! over a connection, which is what lets the app-level MCP server answer with
//! the same types the app draws — and what lets anything outside cydonia read
//! a project without linking the app.
//!
//! Nothing here reaches for gpui, and that is the rule the crate exists to
//! hold: a shape a client reads must not carry anything about a window.

pub mod board;
pub mod chat;
pub mod data;
pub mod project;
pub mod properties;
pub mod record;
