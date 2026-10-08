//! What every front end does between having its settings and opening its first
//! window: fonts, the palette, markdown's hooks, the cover cache and the keymap.

use crate::{
    memory,
    model::{fonts, language, palette, settings::Settings, workspace},
    view::{article, keymap},
};
use bezel::theme::AppExt as _;
use bezel::ui::AppExt as _;
use bezel::{
    gpui::App,
    theme::{Tint, appearance},
    ui,
};
use markdown::AppExt as _;

pub fn init(settings: &Settings, cx: &mut App) {
    if let Err(err) = ui::register_fonts(cx) {
        log_error(&format!("font registration failed: {err:?}"));
    }
    let look = settings.appearance.clone();
    crate::view::component::file::external::init(settings.open_with.clone());
    fonts::set_terminal_caret(look.terminal_caret);
    // Before `appearance::init`, which installs the first palette.
    let inputs = palette::Inputs::of(&look, fonts::Families::of(&look));
    let themed = inputs.theme.is_some();
    palette::register(inputs, cx);
    appearance::init(look.mode, cx);
    // Before the window is opened: it reads its background appearance
    // on the way up, and vibrancy is what decides that.
    workspace::apply_caption_style(look.traffic_lights, cx);
    workspace::apply_transparency(look.vibrancy, look.blur, cx);
    workspace::apply_tint(Tint::new(look.hue, look.chroma), themed, cx);
    cx.set_caret_blink(look.cursor_blink);
    cx.set_caret_shape(look.caret_shape.into());
    cx.set_caret_height(look.caret_height.into());
    cx.set_inactive_caret(crate::model::settings::inactive_caret(look.hollow_caret));
    cx.set_base_text_size(look.text_size);
    workspace::apply_wrap_code(look.wrap_code, cx);
    cx.set_source_style(article::source_style);
    cx.set_marks(article::marks());
    markdown::AppExt::set_link_card(cx, crate::view::entry_link::card);
    editor::AppExt::set_mention_source(cx, '@', crate::view::mention::source);
    article::set_highlight(look.highlight);
    cx.set_mark_paint(article::mark_paint);
    article::set_search(look.search);
    cx.set_find_paint(article::find_paint);
    // The whole catalogue, not the cached subset: the fence picker lists
    // what this list holds, and a picker that offered only what had already
    // been downloaded could not be used to ask for anything else. Naming a
    // language that is not cached is what fetches it — see
    // [`crate::model::language::ensure`].
    cx.set_highlighter(language::highlight, language::offerable());
    #[cfg(feature = "desktop")]
    crate::model::link::init(cx);
    // Without the web fetch, cydonia's own links are the only ones described.
    #[cfg(not(feature = "desktop"))]
    cx.set_link_preview(crate::view::mention::preview);
    cx.set_link_handler(crate::view::entry_link::open_link);
    memory::init(settings.cover_memory * 1_000_000, cx);
    // Every chord in the app, bezel's included — see
    // [`crate::view::keymap`]. One call rather than an `init` per
    // surface, because the reader can move some of them and moving one
    // means putting the whole keymap back together.
    keymap::bind_all(&settings.shortcuts, cx);
}

fn log_error(text: &str) {
    #[cfg(not(target_family = "wasm"))]
    eprintln!("{text}");
    #[cfg(target_family = "wasm")]
    let _ = text;
}
