//! Text cut down to fit a line.

use cydonia_artifact::text;

#[test]
fn a_preview_is_the_words_without_the_markdown() {
    let markdown = "## Setup\n\nRun `cargo build` and **then**\nopen it.\n\n![a cat](assets/cat.png)\n\n- one\n- two";
    assert_eq!(
        text::preview(markdown),
        "Setup Run cargo build and then open it. one two"
    );
}

#[test]
fn a_long_preview_is_cut_with_an_ellipsis() {
    let preview = text::preview(&"word ".repeat(100));
    assert!(preview.ends_with('…'), "{preview}");
    assert!(preview.chars().count() <= 201, "{preview}");
}
