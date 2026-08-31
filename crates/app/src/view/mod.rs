//! What the app draws. Every view here reads
//! [`crate::model::workspace::Workspace`] and writes to it by name; none of
//! them owns app state.

pub mod article;
pub mod board;
pub mod chat;
pub mod composer;
pub mod menu;
pub mod rail;
pub mod root;
pub mod settings;
pub mod table;
pub mod transcript;
