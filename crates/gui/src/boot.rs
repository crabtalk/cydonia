//! What every front end does between having its settings and opening its first
//! window: fonts, the palette, markdown's hooks, the cover cache and the keymap.

use crate::{
    memory,
    model::{fonts, language, settings::Settings, workspace},
    view::{article, keymap},
};
use bezel::{
    gpui::App,
    theme::{self, Tint, appearance},
    ui::{self, input},
};

pub fn init(settings: &Settings, cx: &mut App) {
    if let Err(err) = ui::register_fonts(cx) {
        log_error(&format!("font registration failed: {err:?}"));
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
    markdown::set_marks(cx, article::marks());
    article::set_highlight(look.highlight.color());
    markdown::set_mark_paint(cx, article::mark_paint);
    // The whole catalogue, not the cached subset: the fence picker lists
    // what this list holds, and a picker that offered only what had already
    // been downloaded could not be used to ask for anything else. Naming a
    // language that is not cached is what fetches it — see
    // [`crate::model::language::ensure`].
    markdown::set_highlighter(cx, language::highlight, language::offerable());
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
