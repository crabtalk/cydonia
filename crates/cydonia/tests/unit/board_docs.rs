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

#[test]
fn title_and_preview_share_the_full_parse() {
    let docs = Docs::default();
    let source = "# Ship **it**\n\nThe full description.";
    let full = docs.of(source);
    assert_eq!(docs.title(source).as_ref(), "Ship it");
    docs.preview(source);
    assert!(Rc::ptr_eq(&full, &docs.of(source)));
    assert_eq!(docs.0.borrow().len(), 1);
}

#[test]
fn list_titles_handle_markdown_and_non_prose_cards() {
    let docs = Docs::default();
    for (source, expected) in [
        (
            "- [ ] **Ship** the [release](https://example.com)",
            "Ship the release",
        ),
        ("```rust\nlet answer = 42;\n```", "let answer = 42;"),
        ("| Name | Status |\n| --- | --- |\n| Task | Done |", "Name"),
        ("![A picture](https://example.com/a.png)", "A picture"),
        ("![](https://example.com/a.png)", "Image"),
        ("---\n\n# A heading", "A heading"),
        ("", "Untitled card"),
    ] {
        assert_eq!(docs.title(source).as_ref(), expected, "{source}");
    }
    let long = docs.title(&"界".repeat(1000));
    assert_eq!(long.chars().count(), 513);
    assert!(long.ends_with('…'));
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
