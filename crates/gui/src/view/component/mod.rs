//! The pieces a screen hangs in itself. None of them is a place you can be.

#[cfg(not(target_os = "linux"))]
pub mod browse;
#[cfg(not(target_os = "linux"))]
pub mod browser;
pub mod changes;
pub mod composer;
pub mod meter;
pub mod ribbon;
#[cfg(feature = "desktop")]
pub mod terminal;
pub mod transcript;

pub mod file;
pub mod panel;

pub mod files;

pub mod divider;

pub mod margin;

pub mod status;

mod image_preview;
