//! The frame meter, floating over the app's window.
//!
//! One for the program: a second meter counts the first one's frames, and
//! neither reads zero again. It hangs here rather than on the settings window
//! whose switch turns it on, so closing that window leaves it where you
//! dragged it — and its CPU figure is the whole process either way.

use bezel::{
    gpui::{AnyElement, Entity, Window, point, prelude::*, px},
    ui::{
        floating::{self, Floating},
        stats::{self, Stats},
    },
};

/// The panel's margin from the window's top-trailing corner, until a drag
/// gives it a place of its own.
const INSET: f32 = 16.;

pub(crate) fn panel(
    id: &'static str,
    at: &Floating,
    meter: &Entity<Stats>,
    window: &Window,
) -> AnyElement {
    let home = point(
        px(f32::from(window.viewport_size().width) - stats::WIDTH - INSET),
        px(INSET),
    );
    floating::panel(id, at, home, meter.clone()).into_any_element()
}
