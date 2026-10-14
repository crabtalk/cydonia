//! What goes over the window on its
//! [`ModalLayer`](crate::view::window::modal::ModalLayer), raised from more
//! than one screen.

pub mod confirm;
pub mod desktop;
pub mod install_app;
pub mod new_board;
pub mod palette;
#[cfg(not(feature = "desktop"))]
pub mod settings;
