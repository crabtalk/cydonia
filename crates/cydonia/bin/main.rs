//! Cydonia — desktop client for ACP agents.

// No console window beside the app. Debug builds keep one for their output.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

use anyhow::Result;
use bezel::{gpui::App, gpui_platform};
use gui::{
    agent, boot,
    model::{media, migrate, notify, settings, state, update, welcome},
    view::{hotkey, menubar, root},
};

/// What the system knows this app as, matching `CFBundleIdentifier` in
/// `bundle/Info.plist`. A notification is posted under it, and a build whose
/// identity says nothing posts under nothing.
const BUNDLE_ID: &str = "sh.cydonia";

fn main() -> Result<()> {
    // First of all, and while this is still the only thread: it writes the
    // process environment, and everything downstream of it — an agent spawned
    // by name, an `npm` the installer runs — resolves against what it leaves.
    agent::path::adopt();
    // Ahead of both readers: it moves keys between the two files, and either
    // one read first would be read from before the move.
    migrate::run();
    let settings = settings::load()?;
    let mut state = state::restore();
    // After the restore and before the window: it reads whether `state.toml`
    // is there, which is what tells a first run from every other one.
    welcome::seed(&mut state);
    let app = gpui_platform::application();
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
        // Before any window or notification: it is the name and identity the
        // system presents this app under — see [`gui::model::notify`].
        cx.set_app_identity(BUNDLE_ID, "Cydonia");
        notify::on_response(cx);
        boot::init(&settings, cx);
        // And the one key the app does not hold itself, which is nothing at
        // all until somebody asks for one — see [`gui::view::hotkey`].
        hotkey::apply(settings.shortcuts.activate(), cx);
        // Where a pasted screenshot's bytes go, which is the app's to say.
        media::init(cx);
        // Ahead of the menu bar, which asks whether this build has an updater
        // at all before it puts an item there for one.
        update::init(settings.auto_update, cx);
        // Last: it reads every binding above off the keymap to put the
        // shortcuts beside its items.
        menubar::init(cx);

        root::open(settings, state, cx).expect("failed to open window");
        cx.activate(true);
    });
    Ok(())
}
