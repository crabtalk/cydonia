//! Cydonia — desktop client for ACP agents.

// No console window beside the app. Debug builds keep one for their output.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

use anyhow::Result;
use bezel::{
    gpui::App,
    gpui_platform,
    theme::{self, Tint, appearance},
    ui::{self, input},
};
use cydonia::{
    agent, memory,
    model::{fonts, language, media, migrate, notify, settings, state, update, welcome, workspace},
    view::{article, hotkey, keymap, menubar, root},
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
        // system presents this app under — see [`cydonia::model::notify`].
        cx.set_app_identity(BUNDLE_ID, "Cydonia");
        notify::on_response(cx);
        if let Err(err) = ui::register_fonts(cx) {
            eprintln!("font registration failed: {err:?}");
        }
        let look = settings.appearance.clone();
        // Both before the first palette is installed: the builder reads the
        // families, and `init` is what calls it.
        fonts::init(fonts::Families {
            sans: look.ui_font.clone().map(Into::into),
            body: look.article_font.clone().map(Into::into),
            mono: look.mono_font.clone().map(Into::into),
        });
        theme::set_palette(fonts::palette, cx);
        appearance::init(look.mode, cx);
        // Before the window is opened: it reads its background appearance
        // on the way up, and vibrancy is what decides that.
        workspace::apply_transparency(look.opaque, cx);
        workspace::apply_tint(Tint::new(look.hue, look.chroma), cx);
        input::set_caret_blink(look.cursor_blink, cx);
        theme::set_base_text_size(look.text_size, cx);
        workspace::apply_wrap_code(look.wrap_code, cx);
        markdown::set_source_style(cx, article::source_style);
        // The whole catalogue, not the cached subset: the fence picker lists
        // what this list holds, and a picker that offered only what had already
        // been downloaded could not be used to ask for anything else. Naming a
        // language that is not cached is what fetches it — see
        // [`cydonia::model::language::ensure`].
        markdown::set_highlighter(cx, language::highlight, language::offerable());
        memory::init(settings.cover_memory * 1_000_000, cx);
        // Every chord in the app, bezel's included — see
        // [`cydonia::view::keymap`]. One call rather than an `init` per
        // surface, because the reader can move some of them and moving one
        // means putting the whole keymap back together.
        keymap::bind_all(&settings.shortcuts, cx);
        // And the one key the app does not hold itself, which is nothing at
        // all until somebody asks for one — see [`cydonia::view::hotkey`].
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
