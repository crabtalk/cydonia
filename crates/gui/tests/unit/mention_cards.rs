//! What `@` lists of the boards' cards: handles by prefix in the active
//! project or the one named, then cards whose text holds the query.

use super::*;

fn board(key: &str, texts: &[&str]) -> Board {
    let mut board = Board::new(key.to_lowercase(), key);
    board.key = key.to_owned();
    let column = board.add_column("Todo").id.clone();
    for text in texts {
        board.add_card(&column, (*text).to_owned()).unwrap();
    }
    board
}

fn carded<'a>(project: &str, active: bool, board: &'a Board) -> Vec<Carded<'a>> {
    board
        .columns
        .iter()
        .flat_map(|column| &column.cards)
        .map(|card| Carded {
            project: project.to_owned(),
            active,
            board,
            card,
            handle: board.handle_of(card).unwrap(),
        })
        .collect()
}

fn handles(cards: &[Carded], rows: &[usize]) -> Vec<String> {
    rows.iter().map(|&ix| cards[ix].handle.clone()).collect()
}

#[test]
fn a_handle_prefix_lists_the_active_projects_cards_exact_first() {
    let texts: Vec<String> = (1..=12).map(|n| format!("card {n}")).collect();
    let texts: Vec<&str> = texts.iter().map(String::as_str).collect();
    let here = board("DEV", &texts);
    let there = board("DEV", &["elsewhere"]);
    let mut cards = carded("cydonia", true, &here);
    cards.extend(carded("bezel", false, &there));
    let (named, _) = rank_cards("dev-1", &cards);
    assert_eq!(
        handles(&cards, &named),
        ["DEV-1", "DEV-10", "DEV-11", "DEV-12"]
    );
    let (named, _) = rank_cards("bezel#DEV-", &cards);
    assert_eq!(handles(&cards, &named), ["DEV-1"]);
    assert_eq!(cards[named[0]].project, "bezel");
}

#[test]
fn text_matches_follow_in_every_project_active_first() {
    let here = board("DEV", &["Fix the drag", "Ship it"]);
    let there = board("ROAD", &["drag motion"]);
    let mut cards = carded("bezel", false, &there);
    cards.extend(carded("cydonia", true, &here));
    let (named, holding) = rank_cards("DRAG", &cards);
    assert!(named.is_empty());
    assert_eq!(handles(&cards, &holding), ["DEV-1", "ROAD-1"]);
}

#[test]
fn an_empty_or_prefixed_query_lists_no_cards() {
    let here = board("DEV", &["Fix the drag"]);
    let cards = carded("cydonia", true, &here);
    for query in ["", "  ", "b:drag", "l:ui"] {
        assert_eq!(rank_cards(query, &cards), (vec![], vec![]), "{query:?}");
    }
    assert_eq!(rank_cards("#drag", &cards), (vec![], vec![]));
}

#[test]
fn a_long_card_title_is_cut_at_a_word() {
    assert_eq!(short_card_title("Fix the drag"), "Fix the drag");
    let long = "one file per card: the board file and one document directory per card";
    assert_eq!(
        short_card_title(long),
        "one file per card: the board file and one…"
    );
    let word = "x".repeat(60);
    assert_eq!(short_card_title(&word), format!("{}…", "x".repeat(48)));
}

#[test]
fn a_card_opening_with_a_heading_is_chipped_by_the_whole_heading() {
    let heading = "# Board card editor misses article features and more besides\n\nBody.";
    assert_eq!(
        card_chip_title(heading),
        "Board card editor misses article features and more besides"
    );
    let plain = "one file per card: the board file and one document directory per card";
    assert_eq!(
        card_chip_title(plain),
        "one file per card: the board file and one…"
    );
}
