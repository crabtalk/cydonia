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
