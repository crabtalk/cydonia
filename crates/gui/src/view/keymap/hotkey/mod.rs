//! The one chord the app does not own: the key macOS holds on its behalf, so
//! that pressing it inside some other application brings cydonia forward.
//!
//! Not a keymap binding, and it cannot be one. gpui only sees a keystroke while
//! one of our windows has it, and the whole point of this key is the moment
//! when none of them does. So it is registered with the system instead —
//! Carbon's `RegisterEventHotKey`, underneath [`global_hotkey`] — which asks no
//! permission of the user, unlike a global event monitor, and which is the same
//! door every launcher on the desktop is already queueing at.
//!
//! That queue is the failure this module exists to report. A chord another app
//! registered first is refused outright and says so; a chord macOS itself has
//! claimed — ⌃Space is the input-source switch, and is the one most people
//! reach for here — registers cleanly and then never fires, because the system
//! took it upstream of us. Nothing can detect the second case, which is why
//! nothing here ships on by default: [`crate::model::settings::Shortcuts`]
//! holds no `activate` until somebody asks for one.

use bezel::gpui::{App, SharedString};

#[cfg(feature = "desktop")]
mod system;

/// Where this build holds the activate chord.
#[cfg(feature = "desktop")]
pub type Here = system::System;
#[cfg(not(feature = "desktop"))]
pub type Here = Unheld;

/// The activate chord, held outside the app.
pub trait Hotkey {
    /// Hold `chord`, letting go of whatever was held before. `None` lets go
    /// and takes nothing.
    fn apply(chord: Option<&str>, cx: &mut App);

    /// Why the chord in the file is not being held, when it is not.
    fn refused(cx: &App) -> Option<SharedString>;
}

/// No chord held: a browser tab cannot hear a key while it is not in front.
#[cfg(not(feature = "desktop"))]
pub struct Unheld;

#[cfg(not(feature = "desktop"))]
impl Hotkey for Unheld {
    fn apply(chord: Option<&str>, cx: &mut App) {
        let _ = (chord, cx);
    }

    fn refused(cx: &App) -> Option<SharedString> {
        let _ = cx;
        Some("A browser tab cannot answer a chord while it is in the background.".into())
    }
}
