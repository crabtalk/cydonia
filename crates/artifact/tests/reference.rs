//! The written form of a reference.

use cydonia_artifact::reference::{Reference, Target, Turns, parse};

fn entry(project: Option<&str>, number: u64, turns: Option<(u64, u64)>) -> Option<Reference<'_>> {
    Some(Reference {
        project,
        target: Target::Entry {
            number,
            turns: turns.map(|(from, to)| Turns { from, to }),
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
    ] {
        assert_eq!(parse(text), None, "{text:?}");
    }
}

#[test]
fn a_run_writes_back_as_it_was_read() {
    assert_eq!(Turns { from: 5, to: 5 }.to_string(), "5");
    assert_eq!(Turns { from: 5, to: 7 }.to_string(), "5-7");
}
