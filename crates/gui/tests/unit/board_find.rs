//! What the board's find bar matches on, which is what a card can be named by:
//! its handle and its text.

use super::*;
use artifact::board::Card;

fn card(handle: u64, text: &str) -> Card {
    Card::new(format!("card-{handle}"), handle, text.to_owned())
}

#[test]
fn an_empty_query_keeps_every_card() {
    let card = card(38, "support search in board");
    assert!(card_matches(&card, Some("DEV-38"), ""));
    assert!(card_matches(&card, Some("DEV-38"), "   "));
    assert!(card_matches(&card, None, ""));
}

#[test]
fn text_and_handle_both_match_and_case_is_ignored() {
    let card = card(38, "Support Search in board");
    assert!(card_matches(&card, Some("DEV-38"), "search"));
    assert!(card_matches(&card, Some("DEV-38"), "dev-38"));
    // The query is trimmed, so a chord that left a space still finds the card.
    assert!(card_matches(&card, Some("DEV-38"), " SEARCH "));
    assert!(!card_matches(&card, Some("DEV-38"), "padding"));
}

/// A board with no key hands out no handles — see
/// [`artifact::board::Board::handle_of`] — so there is nothing but the text.
#[test]
fn a_card_without_a_handle_matches_on_its_text_alone() {
    let card = card(38, "support search in board");
    assert!(card_matches(&card, None, "search"));
    assert!(!card_matches(&card, None, "dev-38"));
}
