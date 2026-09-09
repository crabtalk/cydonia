//! Cydonia — desktop client for ACP agents.

// A Windows executable is a console program unless it says otherwise, and a
// console program launched from Explorer is given a console: a black window
// standing behind the app for as long as it runs. Release builds say
// otherwise. Debug builds keep the console, which is where `cargo run` shows
// a panic.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

use anyhow::Result;
use bezel::{
    gpui::App,
    gpui_platform,
    theme::{self, Tint, appearance},
    ui::{self, focus, input},
};
use cydonia::{
    assets, memory,
    model::{settings, state, workspace},
    view::{article, board, component::composer, menubar, root, table},
};

fn main() -> Result<()> {
    let settings = settings::load()?;
    let state = state::restore();
    let app = gpui_platform::application().with_assets(assets::Assets);
    // The Dock icon and a second launch both land here. ⌘W leaves the app
    // running with no window, as it does in every other mac app, so this is
    // the way back to one.
    app.on_reopen(|cx| {
        if cx
            .windows()
            .iter()
            .any(|window| window.downcast::<root::Cydonia>().is_some())
        {
            return;
        }
        let Ok(settings) = settings::load() else {
            return;
        };
        let _ = root::open(settings, state::restore(), cx);
    });
    app.run(move |cx: &mut App| {
        if let Err(err) = ui::register_fonts(cx) {
            eprintln!("font registration failed: {err:?}");
        }
        appearance::init(state.appearance, cx);
        // Before the window is opened: it reads its background appearance
        // on the way up, and vibrancy is what decides that.
        workspace::apply_transparency(state.reduce_transparency, cx);
        workspace::apply_tint(Tint::new(state.hue, state.chroma), cx);
        input::set_caret_blink(state.cursor_blink, cx);
        theme::set_base_text_size(state.text_size, cx);
        markdown::set_highlighter(
            cx,
            |language, code| syntax::highlight(code, language),
            syntax::lang::LANGS.iter().map(|lang| lang.name),
        );
        memory::init(settings.cover_memory * 1_000_000, cx);
        input::init(cx);
        focus::init(cx);
        composer::init(cx);
        editor::init(cx);
        article::init(cx);
        board::init(cx);
        table::init(cx);
        root::init(cx);
        // Last: it reads every binding above off the keymap to put the
        // shortcuts beside its items.
        menubar::init(cx);
        // No Dock to come back from: off macOS a process with no window is
        // not an app anyone can see or reopen, so the last window closing is
        // the app quitting — the door ⌘Q is on a Mac.
        #[cfg(not(target_os = "macos"))]
        cx.on_window_closed(|cx, _| {
            if cx.windows().is_empty() {
                cx.quit();
            }
        })
        .detach();

        root::open(settings, state, cx).expect("failed to open window");
        cx.activate(true);
    });
    Ok(())
}
