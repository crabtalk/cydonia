//! Where in an article's markdown a span or an anchor points.

use cydonia_artifact::{
    article::anchor::{headings, lines, section, slug, span_of},
    reference::Span,
};

const DOC: &str = "# Guide\n\nIntro.\n\n## Set up\n\nStep one.\n\n```sh\n# not a heading\n```\n\n### Details\n\nMore.\n\n## Set up\n\nAgain.\n\n# Next\n\nEnd.\n";

#[test]
fn a_slug_follows_github() {
    assert_eq!(slug("Set Up: the `cli`!"), "set-up-the-cli");
    assert_eq!(slug("snake_case and-dash"), "snake_case-and-dash");
    assert_eq!(slug("Café über"), "café-über");
}

#[test]
fn headings_skip_fences_and_suffix_repeats() {
    let anchors: Vec<_> = headings(DOC).into_iter().map(|h| h.anchor).collect();
    assert_eq!(anchors, ["guide", "set-up", "details", "set-up-1", "next"]);
}

#[test]
fn a_section_runs_to_the_next_heading_at_its_level_or_above() {
    let (heading, range) = section(DOC, "set-up").unwrap();
    assert_eq!(heading.text, "Set up");
    assert_eq!(
        &DOC[range],
        "## Set up\n\nStep one.\n\n```sh\n# not a heading\n```\n\n### Details\n\nMore.\n\n"
    );
    let (_, range) = section(DOC, "next").unwrap();
    assert_eq!(&DOC[range], "# Next\n\nEnd.\n");
    assert_eq!(section(DOC, "missing"), None);
}

#[test]
fn lines_are_whole_and_clip_at_the_end() {
    let text = "one\ntwo\nthree\n";
    assert_eq!(
        &text[lines(text, Span { from: 2, to: 2 }).unwrap()],
        "two\n"
    );
    assert_eq!(
        &text[lines(text, Span { from: 2, to: 9 }).unwrap()],
        "two\nthree\n"
    );
    assert_eq!(
        &text[lines(text, Span { from: 1, to: 1 }).unwrap()],
        "one\n"
    );
    assert_eq!(lines(text, Span { from: 4, to: 4 }), None);
    assert_eq!(
        &"a\nb"[lines("a\nb", Span { from: 2, to: 2 }).unwrap()],
        "b"
    );
}

#[test]
fn a_range_reads_back_as_the_lines_it_covers() {
    let (_, range) = section(DOC, "next").unwrap();
    assert_eq!(span_of(DOC, range), Span { from: 21, to: 23 });
    let text = "one\ntwo\nthree\n";
    let range = lines(text, Span { from: 2, to: 3 }).unwrap();
    assert_eq!(span_of(text, range), Span { from: 2, to: 3 });
}
