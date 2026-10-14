//! What the app draws. Every view here reads
//! [`crate::model::workspace::Workspace`] and writes to it by name; none of
//! them owns app state.

pub mod app;
pub mod article;
pub mod board;
pub mod component;
pub mod detail;
pub mod keymap;
pub mod kit;
pub mod library;
pub mod modals;
pub mod pane;
pub mod search;
pub mod settings;
pub mod sidebar;
pub mod table;
pub mod window;

#[cfg(test)]
#[path = "../../tests/unit/clipboard.rs"]
pub(crate) mod clipboard_tests;
