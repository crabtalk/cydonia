//! What the app draws. Every view here reads
//! [`crate::model::workspace::Workspace`] and writes to it by name; none of
//! them owns app state.

pub mod article;
pub mod board;
pub mod composer;
pub mod root;
pub mod settings_window;
pub mod table;
pub mod transcript;
