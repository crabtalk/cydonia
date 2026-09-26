//! The pieces a screen hangs in itself. Nothing here answers to
//! [`crate::view::root::Pane`]: these are not places you can be.

pub mod changes;
pub mod composer;
pub mod menu;
pub mod meter;
pub mod ribbon;
#[cfg(feature = "desktop")]
pub mod terminal;
pub mod transcript;

pub mod file;
pub mod panel;

pub mod files;

pub mod divider;

pub mod status;

mod image_preview;
