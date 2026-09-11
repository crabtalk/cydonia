//! The artifacts cydonia keeps: the articles, boards, sessions and tables a
//! project holds, as shapes rather than as files.
//!
//! One module per kind, which is the same four the app lets a project show.
//! [`project`] is the odd one out and is meant to be: it is what holds the
//! other four and hands them over, not a fifth thing beside them.
//!
//! The filesystem is one backend for these and not the definition of them. A
//! board is a board whether it was read out of `.cydonia/boards/` or handed
//! over a connection, which is what lets the app-level MCP server answer with
//! the same types the app draws — and what lets anything outside cydonia read
//! a project without linking the app.
//!
//! Nothing here reaches for gpui, and that is the rule the crate exists to
//! hold: a shape a client reads must not carry anything about a window.

pub mod article;
pub mod board;
pub mod id;
pub mod project;
pub mod session;
pub mod stamp;
pub mod table;
