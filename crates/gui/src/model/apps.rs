//! Mini-apps: wasm modules built against `cydonia-guest`, each open view
//! holding one running instance through [`Runner`].

use anyhow::Result;
use std::ops::Range;

pub use guest::wire::{Answer, Call, CallId, Event, Node};

/// What this build runs apps on.
#[cfg(feature = "desktop")]
pub type Runtime = wasmtime::Wasmtime;
#[cfg(not(feature = "desktop"))]
pub type Runtime = none::Absent;

#[cfg(feature = "desktop")]
mod wasmtime;

#[cfg(not(feature = "desktop"))]
mod none;

/// What an app asked of Cydonia during a call, for Cydonia to do once the
/// call has returned.
#[derive(Clone, Debug, PartialEq)]
pub enum Command {
    /// Put `value` in the input `key`.
    SetInput { key: String, value: String },
    /// Do `call`, and deliver its answer as an [`Event::Answer`] numbered `id`.
    Call { id: CallId, call: Call },
}

/// One running instance of an app. A call that fails leaves the instance
/// unusable: the error is the app's last word.
pub trait Runner: Sized + Send + 'static {
    /// Compile `wasm`, instantiate it and run its `init`.
    fn start(wasm: &[u8]) -> Result<(Self, Vec<Command>)>;

    /// Deliver `event`.
    fn update(&mut self, event: &Event) -> Result<Vec<Command>>;

    /// What the app draws now.
    fn view(&mut self) -> Result<Node>;

    /// The rows `range` of the list `list` the last view drew.
    fn rows(&mut self, list: &str, range: Range<u32>) -> Result<Vec<Node>>;
}
