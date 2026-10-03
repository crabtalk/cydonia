//! What the app draws. Every view here reads
//! [`crate::model::workspace::Workspace`] and writes to it by name; none of
//! them owns app state.

pub mod arrangement;
pub mod article;
pub mod board;
pub mod chrome;
pub mod component;
pub mod confirm;
pub mod create;
pub mod desktop;
pub mod detail;
pub mod find;
pub mod header;
#[cfg(feature = "desktop")]
pub mod hotkey;
pub mod info;
pub mod keymap;
pub mod leaf;
pub mod lights;
pub mod mention;
pub mod menubar;
pub mod root;
pub mod search;
pub mod section;
pub mod session_card;
pub mod settings;
pub mod sidebar;
pub mod table;

#[cfg(test)]
#[path = "../../tests/unit/clipboard.rs"]
pub(crate) mod clipboard_tests;
