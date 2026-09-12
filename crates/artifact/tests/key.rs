//! What a board answers to out loud.

use cydonia_artifact::board::key;
use std::collections::HashSet;

fn free() -> HashSet<String> {
    HashSet::new()
}

/// A name of several words keys as its initials; a name of one keys as its
/// opening.
#[test]
fn a_key_is_the_shape_of_the_name() {
    assert_eq!(key::derive("Why CrabTalk", &free()), "WC");
    assert_eq!(key::derive("Roadmap", &free()), "ROA");
    assert_eq!(
        key::derive("a b c d e f", &free()),
        "ABCD",
        "four words at most"
    );
}

/// A key already spoken for takes a digit, which is why a handle splits off its
/// last `-` and not its first.
#[test]
fn a_taken_key_takes_a_digit() {
    let taken: HashSet<String> = ["ROA".to_owned(), "ROA2".to_owned()].into();
    assert_eq!(key::derive("Roadmap", &taken), "ROA3");
}

/// A name with no ASCII in it still has to be sayable.
#[test]
fn a_name_with_no_ascii_still_keys() {
    assert_eq!(key::derive("路线图", &free()), "B");
    let taken: HashSet<String> = ["B".to_owned()].into();
    assert_eq!(key::derive("路线图", &taken), "B2");
}

/// A key as typed: the ASCII of it, uppercased. Nothing for a string that is
/// not a key anyone could say.
#[test]
fn normalizing_keeps_only_what_can_be_typed() {
    assert_eq!(key::normalize(" ro-ad! ").as_deref(), Some("ROAD"));
    assert_eq!(key::normalize("路线图"), None);
    assert_eq!(key::normalize(""), None);
}
