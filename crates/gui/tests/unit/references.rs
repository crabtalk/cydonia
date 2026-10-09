//! Part of an article a reference names, as the blocks its editor numbers.

use super::passage;
use crate::model::article::Disk;
use artifact::reference::{Span, Within};
use std::rc::Rc;

const TEXT: &str = "# Guide\n\nIntro.\n\n## Set up\n\nStep one.\n\n- a\n- b\n\n## Use\n\nGo.\n";

fn disk() -> Rc<Disk> {
    Rc::new(Disk {
        text: TEXT.to_owned(),
        blocks: markdown::parse_ranges(TEXT).block_ranges,
    })
}

#[test]
fn a_heading_covers_its_section_blocks() {
    let found = passage(disk(), Within::Heading("set-up"), "#1#set-up")
        .ok()
        .unwrap();
    assert_eq!(found.blocks, 2..6);
    assert_eq!(found.label, "Set up");
    assert_eq!(found.markdown(), "## Set up\n\nStep one.\n\n- a\n- b\n\n");
}

#[test]
fn lines_cover_every_block_they_touch() {
    let span = Span { from: 3, to: 7 };
    let found = passage(disk(), Within::Span(span), "#1:3-7").ok().unwrap();
    assert_eq!(found.blocks, 1..4);
    assert_eq!(found.label, "lines 3-7");
    let one = Span { from: 10, to: 10 };
    let found = passage(disk(), Within::Span(one), "#1:10").ok().unwrap();
    assert_eq!(found.blocks, 5..6);
    assert_eq!(found.label, "line 10");
}

#[test]
fn a_part_the_article_lacks_is_refused() {
    assert!(passage(disk(), Within::Heading("gone"), "#1#gone").is_err());
    let past = Span { from: 99, to: 99 };
    assert!(passage(disk(), Within::Span(past), "#1:99").is_err());
}
