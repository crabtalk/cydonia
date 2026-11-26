use crate::view::design::{CONTEXT, bindings};
use bezel::{
    gpui::{KeyContext, Keymap, Keystroke},
    ui::input::KEY_CONTEXT,
};

#[test]
fn design_escape_yields_to_the_comment_field() {
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
fn design_zoom_keys_yield_to_the_comment_field() {
    let keymap = Keymap::new(bindings());
    let review = KeyContext::parse(CONTEXT).unwrap();
    let text = KeyContext::parse(KEY_CONTEXT).unwrap();

    let keys = if cfg!(target_os = "macos") {
        ["cmd-=", "cmd--", "cmd-0"]
    } else {
        ["ctrl-=", "ctrl--", "ctrl-0"]
    };
    for key in keys {
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
