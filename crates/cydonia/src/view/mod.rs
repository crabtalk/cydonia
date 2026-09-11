//! What the app draws. Every view here reads
//! [`crate::model::workspace::Workspace`] and writes to it by name; none of
//! them owns app state.

pub mod article;
pub mod board;
pub mod component;
pub mod detail;
pub mod header;
pub mod menubar;
pub mod root;
pub mod settings;
pub mod sidebar;
pub mod table;
