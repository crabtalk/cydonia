use crate::view::design::{CONTEXT, bindings};
use bezel::{
    gpui::{KeyContext, Keymap, Keystroke},
    ui::input::KEY_CONTEXT,
};

#[test]
fn design_escape_yields_to_text_fields() {
    let keymap = Keymap::new(bindings());
    let review = KeyContext::parse(CONTEXT).unwrap();
    let text = KeyContext::parse(KEY_CONTEXT).unwrap();

    let input = [Keystroke::parse("escape").unwrap()];
    assert_eq!(
        keymap
            .bindings_for_input(&input, std::slice::from_ref(&review))
            .0
            .len(),
        1
    );
    assert!(
        keymap
            .bindings_for_input(&input, &[review, text])
            .0
            .is_empty()
    );
}

#[test]
fn design_tool_and_zoom_keys_yield_to_text_fields() {
    let keymap = Keymap::new(bindings());
    let review = KeyContext::parse(CONTEXT).unwrap();
    let text = KeyContext::parse(KEY_CONTEXT).unwrap();

    let zoom = if cfg!(target_os = "macos") {
        ["cmd-=", "cmd--"]
    } else {
        ["ctrl-=", "ctrl--"]
    };
    for key in zoom.into_iter().chain(["shift-1", "shift-0", "v", "h"]) {
        let input = [Keystroke::parse(key).unwrap()];
        assert_eq!(
            keymap
                .bindings_for_input(&input, std::slice::from_ref(&review))
                .0
                .len(),
            1,
            "{key}"
        );
        assert!(
            keymap
                .bindings_for_input(&input, &[review.clone(), text.clone()])
                .0
                .is_empty(),
            "{key}"
        );
    }
}
