//! Write a Cydonia app.
//!
//! An app is a Rust crate built for `wasm32-unknown-unknown` as a `cdylib`.
//! It implements [`App`] and names its type in [`app!`], which writes the
//! exports Cydonia calls. Cydonia holds one instance of the app per open view:
//! the app's state lives in its own memory between calls.
//!
//! ```ignore
//! use cydonia_guest::{App, node::*};
//!
//! struct Counter(u32);
//!
//! #[derive(Clone)]
//! enum Msg { Add }
//!
//! impl App for Counter {
//!     type Msg = Msg;
//!     fn init() -> Self { Counter(0) }
//!     fn update(&mut self, Msg::Add: Msg) { self.0 += 1 }
//!     fn view(&self) -> Node<Msg> {
//!         row([text(self.0.to_string()).into(), button("Add").on_click(Msg::Add).into()]).into()
//!     }
//! }
//!
//! cydonia_guest::app!(Counter);
//! ```

pub mod fetch;
pub mod host;
pub mod manifest;
pub mod node;
pub mod tables;
pub mod wire;

use node::{Handle, Handlers, Node};
use std::ops::Range;

/// The version of the node and event vocabulary this crate writes. Nodes and
/// fields are only ever added to it.
pub const CONTRACT: u32 = 1;

/// The names of what an app exports and imports, as Cydonia looks them up.
pub mod abi {
    /// The module an app's imports are in.
    pub const MODULE: &str = "cydonia";
    pub const MEMORY: &str = "memory";
    /// `() -> ()`: build the app. Called once, first.
    pub const INIT: &str = "cydonia_init";
    /// `(len: u32) -> ptr: u32`: room for `len` bytes, read by the next call.
    pub const BUFFER: &str = "cydonia_buffer";
    /// `() -> ()`: the buffer holds a JSON [`crate::wire::Event`].
    pub const UPDATE: &str = "cydonia_update";
    /// `() -> u64`: the JSON [`crate::wire::Node`] the app draws, as its
    /// pointer in the high half and its length in the low half. Valid until the
    /// next call.
    pub const VIEW: &str = "cydonia_view";
    /// `(start: u32, end: u32) -> u64`: the buffer holds a list's key; answers
    /// the JSON array of that list's rows `start..end`, as [`VIEW`] answers.
    pub const ROWS: &str = "cydonia_rows";
    /// Import `(key, key_len, value, value_len)`: [`crate::host::set_input`].
    pub const SET_INPUT: &str = "set_input";
    /// Import `(message, len)`: the message of a panic about to trap.
    pub const PANIC: &str = "panic";
    /// Import `(id, request, len)`: the JSON [`crate::wire::Call`] numbered
    /// `id`, answered by an [`crate::wire::Event::Answer`].
    pub const CALL: &str = "call";
}

/// An app: its state, the messages its nodes send, and what it draws.
pub trait App: Sized + 'static {
    type Msg: 'static;

    fn init() -> Self;

    fn update(&mut self, msg: Self::Msg);

    fn view(&self) -> Node<Self::Msg>;

    /// The rows `range` of the list `list` that [`App::view`] drew.
    fn rows(&self, list: &str, range: Range<u32>) -> Vec<Node<Self::Msg>> {
        let _ = (list, range);
        Vec::new()
    }
}

/// An app and the handlers of what it last drew. [`app!`] holds one and calls
/// it from the exports.
pub struct Runtime<A: App> {
    app: A,
    handlers: Handlers<A::Msg>,
    buffer: Vec<u8>,
    out: Vec<u8>,
}

impl<A: App> Runtime<A> {
    pub fn start() -> Self {
        host::hook_panics();
        Self {
            app: A::init(),
            handlers: Handlers::default(),
            buffer: Vec::new(),
            out: Vec::new(),
        }
    }

    pub fn buffer(&mut self, len: u32) -> *mut u8 {
        self.buffer.clear();
        self.buffer.resize(len as usize, 0);
        self.buffer.as_mut_ptr()
    }

    /// Deliver the event in the buffer. An event for a handler no longer drawn,
    /// or an answer to no call waiting, is dropped.
    pub fn update(&mut self) {
        let event: wire::Event =
            serde_json::from_slice(&self.buffer).expect("Cydonia sends a JSON event");
        let msg = match event {
            wire::Event::Click { handler } => match self.handlers.get(handler) {
                Some(Handle::Click(msg)) => msg(),
                _ => return,
            },
            wire::Event::Input { handler, text }
            | wire::Event::Select {
                handler,
                value: text,
            } => match self.handlers.get(handler) {
                Some(Handle::Text(msg)) => msg(text),
                _ => return,
            },
            wire::Event::Answer { call, answer } => match host::answered(call, answer) {
                Some(msg) => msg,
                None => return,
            },
        };
        self.app.update(msg);
    }

    pub fn view(&mut self) -> u64 {
        let node = self.app.view();
        let node = self.handlers.view(node);
        self.answer(&node)
    }

    pub fn rows(&mut self, start: u32, end: u32) -> u64 {
        let list = String::from_utf8(std::mem::take(&mut self.buffer))
            .expect("Cydonia sends a list's key as UTF-8");
        let rows = self.app.rows(&list, start..end);
        let rows = self.handlers.rows(&list, rows);
        self.answer(&rows)
    }

    fn answer(&mut self, value: &impl serde::Serialize) -> u64 {
        self.out = serde_json::to_vec(value).expect("nodes serialize");
        ((self.out.as_ptr() as usize as u64) << 32) | self.out.len() as u64
    }
}

/// Export `$app` to Cydonia, with its [`manifest::Manifest`]. Once, in the
/// app's crate root.
///
/// ```ignore
/// cydonia_guest::app!(Yc, name = "Y Combinator", calls = [tables], hosts = ["yc-oss.github.io"]);
/// ```
///
/// `name` defaults to the crate's name; `calls` names the groups in
/// [`manifest::calls`] the app makes, and `hosts` the hosts it fetches from.
#[macro_export]
macro_rules! app {
    (
        $app:ty
        $(, name = $name:literal)?
        $(, calls = [$($call:ident),* $(,)?])?
        $(, hosts = [$($host:literal),* $(,)?])?
        $(,)?
    ) => {
        #[cfg(target_arch = "wasm32")]
        const _: () = {
            const DECLARED: $crate::manifest::Declared<'static> = $crate::manifest::Declared {
                id: ::std::env!("CARGO_PKG_NAME"),
                name: $crate::__first!($($name)? ; ::std::env!("CARGO_PKG_NAME")),
                version: ::std::env!("CARGO_PKG_VERSION"),
                calls: &[$($(::std::stringify!($call)),*)?],
                hosts: &[$($($host),*)?],
            };
            const SIZE: usize = DECLARED.size();
            #[unsafe(link_section = "cydonia.manifest")]
            #[used]
            static MANIFEST: [u8; SIZE] = DECLARED.bytes::<SIZE>();
        };

        // A native build of the app's crate exports nothing; this keeps the
        // app in use there.
        #[cfg(not(target_arch = "wasm32"))]
        const _: fn() -> $crate::Runtime<$app> = $crate::Runtime::<$app>::start;

        #[cfg(target_arch = "wasm32")]
        const _: () = {
            ::std::thread_local! {
                static RUNTIME: ::std::cell::RefCell<::std::option::Option<$crate::Runtime<$app>>> =
                    const { ::std::cell::RefCell::new(::std::option::Option::None) };
            }

            fn with<R>(call: impl FnOnce(&mut $crate::Runtime<$app>) -> R) -> R {
                RUNTIME
                    .with_borrow_mut(|held| call(held.as_mut().expect("cydonia_init runs first")))
            }

            #[unsafe(no_mangle)]
            extern "C" fn cydonia_init() {
                let runtime = $crate::Runtime::<$app>::start();
                RUNTIME.with_borrow_mut(|held| *held = ::std::option::Option::Some(runtime));
            }

            #[unsafe(no_mangle)]
            extern "C" fn cydonia_buffer(len: u32) -> *mut u8 {
                with(|runtime| runtime.buffer(len))
            }

            #[unsafe(no_mangle)]
            extern "C" fn cydonia_update() {
                with(|runtime| runtime.update())
            }

            #[unsafe(no_mangle)]
            extern "C" fn cydonia_view() -> u64 {
                with(|runtime| runtime.view())
            }

            #[unsafe(no_mangle)]
            extern "C" fn cydonia_rows(start: u32, end: u32) -> u64 {
                with(|runtime| runtime.rows(start, end))
            }
        };
    };
}

#[doc(hidden)]
#[macro_export]
macro_rules! __first {
    ($given:expr ; $default:expr) => {
        $given
    };
    (; $default:expr) => {
        $default
    };
}
