//! Cydonia in a browser tab: the same window the desktop app opens, on gpui's
//! web platform, over the welcome project held in memory.
//!
//! Single-threaded, like bezel's gallery. Sessions are read back and never
//! connect; there is no terminal, settings window, updater or tool server —
//! see `cydonia-gui`'s `desktop` feature.

#![cfg(target_family = "wasm")]

use artifact::project::memory;
use bezel::gpui::{App, Application, ApplicationHandle};
use gui::{
    boot,
    model::{language, settings::Settings, state::State, store, welcome},
    view::root,
};
use std::{cell::RefCell, rc::Rc, sync::Arc};
use wasm_bindgen::prelude::wasm_bindgen;

include!(concat!(env!("OUT_DIR"), "/prepared.rs"));

/// Where the welcome project stands. Nothing is on a disk there: the path is
/// the project's name in the sidebar and the key [`store::seed`] files it
/// under.
const PROJECT: &str = "/welcome";

thread_local! {
    /// The whole app, and the reason it stays alive: on wasm the run loop is
    /// the browser's, so `run_embedded` returns at once and hands back the one
    /// owner of everything it built.
    static APPLICATION: RefCell<Option<ApplicationHandle>> = const { RefCell::new(None) };
}

#[wasm_bindgen(start)]
pub fn start() {
    console_error_panic_hook::set_once();
    gpui_web::init_logging();
    language::prepare(PREPARED);
    store::seed(PROJECT, memory::Project::seed(welcome::files()));

    let settings = Settings::default();
    let state = State {
        projects: vec![PROJECT.into()],
        ..State::default()
    };
    let platform = Rc::new(gpui_web::WebPlatform::new(false));
    let http_client = Arc::new(platform.fetch_http_client());
    let handle = Application::with_platform(platform)
        .with_http_client(http_client)
        .run_embedded(move |cx: &mut App| {
            boot::init(&settings, cx);
            root::open(settings, state, cx).expect("failed to open the window");
        });
    APPLICATION.with(|application| *application.borrow_mut() = Some(handle));
}
