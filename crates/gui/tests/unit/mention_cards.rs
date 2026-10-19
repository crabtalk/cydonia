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

fn carded<'a>(project: &'a str, active: bool, board: &'a Board) -> Vec<Carded<'a>> {
    board
        .columns
        .iter()
        .flat_map(|column| &column.cards)
        .map(|card| Carded {
            project: project.to_owned(),
            path: std::path::Path::new(project),
            active,
            board,
            card,
            number: card.handle.unwrap(),
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
    let (named, _) = Query::parse("dev-1").rank_cards(&cards);
    assert_eq!(
        handles(&cards, &named),
        ["DEV-1", "DEV-10", "DEV-11", "DEV-12"]
    );
    let (named, _) = Query::parse("bezel#DEV-").rank_cards(&cards);
    assert_eq!(handles(&cards, &named), ["DEV-1"]);
    assert_eq!(cards[named[0]].project, "bezel");
}

#[test]
fn a_bare_number_names_the_active_projects_cards_of_that_handle() {
    let texts: Vec<String> = (1..=12).map(|n| format!("card {n}")).collect();
    let texts: Vec<&str> = texts.iter().map(String::as_str).collect();
    let here = board("DEV", &texts);
    let road = board("ROAD", &["one", "two"]);
    let there = board("DEV", &["elsewhere", "also"]);
    let mut cards = carded("cloud", true, &here);
    cards.extend(carded("cloud", true, &road));
    cards.extend(carded("bezel", false, &there));
    let (named, _) = Query::parse("2").rank_cards(&cards);
    assert_eq!(handles(&cards, &named), ["DEV-2", "ROAD-2"]);
    assert!(named.iter().all(|&ix| cards[ix].project == "cloud"));
    let (named, _) = Query::parse("#2").rank_cards(&cards);
    assert!(named.is_empty());
}

#[test]
fn text_matches_follow_in_every_project_active_first() {
    let here = board("DEV", &["Fix the drag", "Ship it"]);
    let there = board("ROAD", &["drag motion"]);
    let mut cards = carded("bezel", false, &there);
    cards.extend(carded("cydonia", true, &here));
    let (named, holding) = Query::parse("DRAG").rank_cards(&cards);
    assert!(named.is_empty());
    assert_eq!(handles(&cards, &holding), ["DEV-1", "ROAD-1"]);
}

#[test]
fn an_empty_or_prefixed_query_lists_no_cards() {
    let here = board("DEV", &["Fix the drag"]);
    let cards = carded("cydonia", true, &here);
    for query in ["", "  ", "b:drag", "l:ui"] {
        assert_eq!(
            Query::parse(query).rank_cards(&cards),
            (vec![], vec![]),
            "{query:?}"
        );
    }
    assert_eq!(Query::parse("#drag").rank_cards(&cards), (vec![], vec![]));
}

#[test]
fn a_long_card_title_is_cut_at_a_word() {
    assert_eq!(
        markdown::parse("Fix the drag").short_title(),
        "Fix the drag"
    );
    let long = "one file per card: the board file and one document directory per card";
    assert_eq!(
        markdown::parse(long).short_title(),
        "one file per card: the board file and one…"
    );
    let word = "x".repeat(60);
    assert_eq!(
        markdown::parse(&word).short_title(),
        format!("{}…", "x".repeat(48))
    );
}

#[test]
fn a_card_opening_with_a_heading_is_chipped_by_the_whole_heading() {
    let heading = "# Board card editor misses article features and more besides\n\nBody.";
    assert_eq!(
        markdown::parse(heading).chip_title(),
        "Board card editor misses article features and more besides"
    );
    let plain = "one file per card: the board file and one document directory per card";
    assert_eq!(
        markdown::parse(plain).chip_title(),
        "one file per card: the board file and one…"
    );
}

#[test]
fn a_bare_number_names_the_entry_of_that_number_before_cards() {
    let entry = |number: u64, title: &str| Linkable {
        icon: kind_icon(Kind::Article),
        title: title.to_owned(),
        about: format!("Article · #{number}"),
        url: format!("cydonia://cloud#{number}"),
        touched: 0,
        project: "cloud".to_owned(),
        active: true,
        number,
        key: None,
        labels: Vec::new(),
        agent: None,
        archived: false,
        kind: Kind::Article,
        row: Row::Entry {
            project: std::path::Path::new("cloud").into(),
            showing: crate::model::workspace::Showing::Article(number.to_string()),
        },
    };
    let held = vec![entry(21, "Later"), entry(2, "Design")];
    let here = board("DEV", &["one", "two"]);
    let cards = carded("cloud", true, &here);
    let (named, _) = Query::parse("2").rank_named(&held, &cards);
    assert_eq!(named, [Named::Entry(1), Named::Card(1), Named::Entry(0)]);
    let (named, _) = Query::parse("dev-2").rank_named(&held, &cards);
    assert_eq!(named[0], Named::Card(1));
}
