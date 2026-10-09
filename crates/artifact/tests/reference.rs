//! The written form of a reference.

use cydonia_artifact::reference::{
    Partial, Prefix, Reference, Span, Target, Within, parse, partial,
};

fn entry(project: Option<&str>, number: u64, turns: Option<(u64, u64)>) -> Option<Reference<'_>> {
    Some(Reference {
        project,
        target: Target::Entry {
            number,
            within: turns.map(|(from, to)| Within::Span(Span { from, to })),
        },
    })
}

fn heading<'a>(project: Option<&'a str>, number: u64, anchor: &'a str) -> Option<Reference<'a>> {
    Some(Reference {
        project,
        target: Target::Entry {
            number,
            within: Some(Within::Heading(anchor)),
        },
    })
}

fn card<'a>(project: Option<&'a str>, key: &'a str, handle: u64) -> Option<Reference<'a>> {
    Some(Reference {
        project,
        target: Target::Card { key, handle },
    })
}

#[test]
fn every_form_in_the_spec_parses() {
    assert_eq!(parse("#43"), entry(None, 43, None));
    assert_eq!(parse("#43:5"), entry(None, 43, Some((5, 5))));
    assert_eq!(parse("#43:5-7"), entry(None, 43, Some((5, 7))));
    assert_eq!(parse("DEV-12"), card(None, "DEV", 12));
    assert_eq!(parse("foo#43"), entry(Some("foo"), 43, None));
    assert_eq!(parse("foo#43:5-7"), entry(Some("foo"), 43, Some((5, 7))));
    assert_eq!(parse("foo#DEV-12"), card(Some("foo"), "DEV", 12));
    assert_eq!(
        parse("cydonia://foo#43:5-7"),
        entry(Some("foo"), 43, Some((5, 7)))
    );
    assert_eq!(parse("cydonia://#43"), entry(None, 43, None));
    assert_eq!(parse("cydonia://resources/board"), None);
    assert_eq!(parse("#12#setup"), heading(None, 12, "setup"));
    assert_eq!(
        parse("foo#12#set-up_2"),
        heading(Some("foo"), 12, "set-up_2")
    );
    assert_eq!(
        parse("cydonia://foo#12#café"),
        heading(Some("foo"), 12, "café")
    );
}

/// A key may end in a digit, so a handle splits at its last dash.
#[test]
fn a_handle_splits_at_its_last_dash() {
    assert_eq!(parse("ROA2-5"), card(None, "ROA2", 5));
    assert_eq!(parse("my-app#ROA2-5"), card(Some("my-app"), "ROA2", 5));
}

#[test]
fn malformed_text_is_not_a_reference() {
    for text in [
        "",
        "#",
        "43",
        "#0",
        "#-1",
        "#43:",
        "#43:0",
        "#43:7-5",
        "#43:5-",
        "#43:-5",
        "#43:5:6",
        "# 43",
        "#43 ",
        "DEV-",
        "-12",
        "DEV-0",
        "DEV 12",
        "#DEV-12:5",
        "my notes#43",
        "a:b#43",
        "a#b#43",
        "hello",
        "#+5",
        "#12#",
        "#12#Set up",
        "#12#a:b",
        "#12#a#b",
        "#12:5#a",
        "#DEV-12#a",
    ] {
        assert_eq!(parse(text), None, "{text:?}");
    }
}

#[test]
fn a_run_writes_back_as_it_was_read() {
    assert_eq!(Span { from: 5, to: 5 }.to_string(), "5");
    assert_eq!(Span { from: 5, to: 7 }.to_string(), "5-7");
}

fn typed<'a>(project: Option<&'a str>, target: Prefix<'a>) -> Option<Partial<'a>> {
    Some(Partial { project, target })
}

#[test]
fn the_start_of_a_reference_reads_as_far_as_it_is_written() {
    assert_eq!(partial("#"), typed(None, Prefix::Number("")));
    assert_eq!(partial("#4"), typed(None, Prefix::Number("4")));
    assert_eq!(partial("4"), typed(None, Prefix::Number("4")));
    assert_eq!(partial("bezel#"), typed(Some("bezel"), Prefix::Number("")));
    assert_eq!(
        partial("bezel#11"),
        typed(Some("bezel"), Prefix::Number("11"))
    );
    assert_eq!(
        partial("cydonia://bezel#1"),
        typed(Some("bezel"), Prefix::Number("1"))
    );
    assert_eq!(partial("DE"), typed(None, Prefix::Key("DE")));
    assert_eq!(partial("bezel#DE"), typed(Some("bezel"), Prefix::Key("DE")));
}

#[test]
fn text_no_reference_starts_with_is_none() {
    for text in [
        "#4:",
        "DEV-",
        "my notes#4",
        "a:b#4",
        "a#b#4",
        "#4 ",
        "two words",
    ] {
        assert_eq!(partial(text), None, "{text:?}");
    }
}
