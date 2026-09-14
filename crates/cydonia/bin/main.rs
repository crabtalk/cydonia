//! Cydonia — desktop client for ACP agents.

use anyhow::Result;
use bezel::{
    gpui::App,
    gpui_platform,
    theme::{self, Tint, appearance},
    ui::{self, focus, input},
};
use cydonia::{
    memory,
    model::{media, migrate, settings, state, update, workspace},
    view::{
        article, board,
        component::{composer, ribbon},
        create, info, menubar, root, table,
    },
};

fn main() -> Result<()> {
    // Ahead of both readers: it moves keys between the two files, and either
    // one read first would be read from before the move.
    migrate::run();
    let settings = settings::load()?;
    let state = state::restore();
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
        if let Err(err) = ui::register_fonts(cx) {
            eprintln!("font registration failed: {err:?}");
        }
        let look = settings.appearance;
        appearance::init(look.mode, cx);
        // Before the window is opened: it reads its background appearance
        // on the way up, and vibrancy is what decides that.
        workspace::apply_transparency(look.opaque, cx);
        workspace::apply_tint(Tint::new(look.hue, look.chroma), cx);
        input::set_caret_blink(look.cursor_blink, cx);
        theme::set_base_text_size(look.text_size, cx);
        workspace::apply_wrap_code(look.wrap_code, cx);
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
        // Where a pasted screenshot's bytes go, which is the app's to say.
        media::init(cx);
        article::init(cx);
        ribbon::init(cx);
        board::init(cx);
        info::init(cx);
        create::init(cx);
        table::init(cx);
        root::init(cx);
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
