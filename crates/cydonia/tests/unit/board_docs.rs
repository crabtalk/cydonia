//! A card's text is parsed once and read back, however many frames draw it.

use super::*;

#[test]
fn one_parse_serves_every_frame_that_draws_the_card() {
    let docs = Docs::default();
    let first = docs.of("- [ ] ship it");
    let again = docs.of("- [ ] ship it");
    assert!(
        Rc::ptr_eq(&first, &again),
        "the second frame reads the parse the first did"
    );
    assert_eq!(*first, markdown::parse("- [ ] ship it"));
}

/// The lane draws the whole card and a list row draws its first line, so the
/// two texts are two entries rather than one that each evicts in turn.
#[test]
fn a_lane_and_a_list_row_hold_their_own_parses() {
    let docs = Docs::default();
    let whole = docs.of("ship it\n\nand the rest of the card");
    let line = docs.of("ship it");
    assert!(!Rc::ptr_eq(&whole, &line));
    assert!(Rc::ptr_eq(
        &whole,
        &docs.of("ship it\n\nand the rest of the card")
    ));
    assert!(Rc::ptr_eq(&line, &docs.of("ship it")));
}

/// Edited text is a key nothing asked for before, so nothing has to be told to
/// forget the old one.
#[test]
fn edited_text_parses_again() {
    let docs = Docs::default();
    let before = docs.of("ship it");
    let after = docs.of("ship it soon");
    assert!(!Rc::ptr_eq(&before, &after));
    assert_eq!(*after, markdown::parse("ship it soon"));
}
