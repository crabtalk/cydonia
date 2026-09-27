//! How a palette row cuts the line its match is on.

use super::*;

#[test]
fn a_short_line_is_kept_whole_with_the_match_where_it_was() {
    let (text, at) = clipped("  the pump runs", 6..10);
    assert_eq!(text.as_ref(), "the pump runs");
    assert_eq!(&text[at], "pump");
}

#[test]
fn a_long_line_is_cut_ahead_of_the_match_and_says_so() {
    let line = format!("{}needle and after", "word ".repeat(40));
    let start = line.find("needle").unwrap();
    let (text, at) = clipped(&line, start..start + 6);
    assert!(text.starts_with('…'), "{text}");
    assert_eq!(&text[at], "needle");
}

#[test]
fn the_title_outranks_the_body_and_earlier_blocks_outrank_later() {
    assert!(rank(&Block::Title) < rank(&Block::Line(0)));
    assert!(rank(&Block::Chat(1)) < rank(&Block::Chat(4)));
}
