//! Carrying an article to another project, and a card to another board.

mod common;

use common::Scratch;
use cydonia_artifact::{article, board, entry, project::fs::Project};
use std::fs;

/// Put an article in a project, with `text` as its document. Answers the path
/// of the `content.md`, which is what a move takes.
fn write_article(scratch: &Scratch, text: &str) -> std::path::PathBuf {
    let dir = article::init(scratch.path()).unwrap();
    let home = article::free(&dir, cydonia_artifact::stamp::now());
    fs::create_dir_all(&home).unwrap();
    let content = article::content(&home);
    fs::write(&content, text).unwrap();
    content
}

/// The document, the cover beside it and the properties all arrive, and the
/// source keeps none of them.
#[test]
fn an_article_arrives_whole_and_leaves_nothing_behind() {
    let here = Scratch::new("move-article-here");
    let there = Scratch::new("move-article-there");
    let content = write_article(&here, "# Roadmap\n");
    let home = content.parent().unwrap().to_owned();
    fs::write(home.join("cover-7.svg"), "<svg/>").unwrap();
    fs::write(home.join("properties.json"), "{}").unwrap();

    let arrived = article::move_to(&content, there.path()).unwrap();

    assert_eq!(fs::read_to_string(&arrived).unwrap(), "# Roadmap\n");
    let landed = arrived.parent().unwrap();
    assert!(landed.join("cover-7.svg").is_file());
    assert!(landed.join("properties.json").is_file());
    assert!(landed.starts_with(there.path()));
    assert!(!home.exists());
}

/// A number is the project's, so the one an article had is tombstoned where it
/// was and a fresh one is issued where it lands.
#[test]
fn a_number_does_not_travel_with_the_article() {
    let here = Scratch::new("move-number-here");
    let there = Scratch::new("move-number-there");
    let content = write_article(&here, "x");
    let id = article::id_of(&content);
    let was = entry::Registry::open(here.path())
        .unwrap()
        .number("article", &id)
        .unwrap();

    let arrived = article::move_to(&content, there.path()).unwrap();

    // Gone from the source, and its number is not handed to the next entry.
    assert!(
        entry::Registry::open(here.path())
            .unwrap()
            .resolve("article", was)
            .unwrap()
            .is_none()
    );
    let now = entry::Registry::open(there.path())
        .unwrap()
        .number("article", &article::id_of(&arrived))
        .unwrap();
    assert_eq!(now, 1);
}

/// The pictures in the body are copied across and the document rewritten, so a
/// moved article does not read out of the project it left.
#[test]
fn the_pictures_in_the_body_come_along() {
    let here = Scratch::new("move-assets-here");
    let there = Scratch::new("move-assets-there");
    let assets = Project::new(here.path()).assets();
    fs::create_dir_all(&assets).unwrap();
    fs::write(assets.join("media-ab.png"), "bytes").unwrap();
    let body = format!("![shot]({}/media-ab.png)\n", assets.display());
    let content = write_article(&here, &body);

    let arrived = article::move_to(&content, there.path()).unwrap();

    let landed = Project::new(there.path()).assets();
    assert_eq!(fs::read(landed.join("media-ab.png")).unwrap(), b"bytes");
    let text = fs::read_to_string(&arrived).unwrap();
    assert!(text.contains(&landed.display().to_string()), "{text}");
    assert!(!text.contains(&assets.display().to_string()), "{text}");
}

/// An id the destination is already using is not a reason to refuse the move:
/// the article takes a free name there and keeps its document.
#[test]
fn an_id_already_taken_is_landed_beside() {
    let here = Scratch::new("move-clash-here");
    let there = Scratch::new("move-clash-there");
    let content = write_article(&here, "mine");
    let id = article::id_of(&content);
    let taken = article::init(there.path()).unwrap().join(&id);
    fs::create_dir_all(&taken).unwrap();
    fs::write(article::content(&taken), "theirs").unwrap();

    let arrived = article::move_to(&content, there.path()).unwrap();

    assert_eq!(fs::read_to_string(&arrived).unwrap(), "mine");
    assert_eq!(
        fs::read_to_string(article::content(&taken)).unwrap(),
        "theirs"
    );
    assert_ne!(article::id_of(&arrived), id);
}

/// A move to the project the article is already in is not a move.
#[test]
fn an_article_does_not_move_to_where_it_is() {
    let here = Scratch::new("move-noop");
    let content = write_article(&here, "x");
    assert_eq!(article::move_to(&content, here.path()).unwrap(), content);
    assert!(content.is_file());
}

// ── cards ────────────────────────────────────────────────────────

fn board(name: &str, key: &str, columns: &[&str]) -> board::Board {
    let mut board = board::Board::new(cydonia_artifact::id::mint(), name);
    board.key = key.to_owned();
    for column in columns {
        board.add_column(column);
    }
    board
}

/// A card takes the destination's handle and lands in the lane of the name it
/// came out of.
#[test]
fn a_card_takes_the_handle_of_the_board_it_lands_on() {
    let mut from = board("Roadmap", "ROAD", &["TODO", "DOING"]);
    let mut to = board("Plans", "PLAN", &["DOING", "TODO"]);
    let doing = from.columns[1].id.clone();
    let card = from.add_card(&doing, "ship it".into()).unwrap().id.clone();

    let landed = board::carry_card(&mut from, &mut to, &card, None).unwrap();

    assert_eq!(landed, "PLAN-1");
    assert!(from.columns.iter().all(|column| column.cards.is_empty()));
    // The lane of the same name, which is not the lane of the same position.
    assert_eq!(to.columns[0].name, "DOING");
    assert_eq!(to.columns[0].cards.len(), 1);
    assert_eq!(to.columns[0].cards[0].text, "ship it");
}

/// The session stays behind: it runs where its board was, and a card that has
/// moved is one to run again.
#[test]
fn a_card_leaves_its_session_behind() {
    let mut from = board("Roadmap", "ROAD", &["TODO"]);
    let mut to = board("Plans", "PLAN", &["TODO"]);
    let todo = from.columns[0].id.clone();
    let card = from.add_card(&todo, "ship it".into()).unwrap().id.clone();
    assert!(from.dispatch_card(&card, "session-1".into()));

    board::carry_card(&mut from, &mut to, &card, None).unwrap();

    assert_eq!(to.columns[0].cards[0].session, None);
}

/// A named lane wins over the one the card came out of.
#[test]
fn a_named_lane_is_where_it_lands() {
    let mut from = board("Roadmap", "ROAD", &["TODO"]);
    let mut to = board("Plans", "PLAN", &["TODO", "DONE"]);
    let todo = from.columns[0].id.clone();
    let card = from.add_card(&todo, "ship it".into()).unwrap().id.clone();
    let done = to.columns[1].id.clone();

    board::carry_card(&mut from, &mut to, &card, Some(&done)).unwrap();

    assert_eq!(to.columns[1].cards.len(), 1);
    assert!(to.columns[0].cards.is_empty());
}

/// Nowhere to land is not a card dropped on the floor.
#[test]
fn a_board_with_no_lanes_keeps_the_card_where_it_was() {
    let mut from = board("Roadmap", "ROAD", &["TODO"]);
    let mut to = board("Plans", "PLAN", &[]);
    let todo = from.columns[0].id.clone();
    let card = from.add_card(&todo, "ship it".into()).unwrap().id.clone();

    assert_eq!(board::carry_card(&mut from, &mut to, &card, None), None);
    assert_eq!(from.columns[0].cards.len(), 1);
}

/// An article's own `assets/` rides along inside its directory, and the whole
/// paths the body holds into it are rewritten to where it landed.
#[test]
fn the_articles_own_pictures_come_along() {
    let here = Scratch::new("move-local-here");
    let there = Scratch::new("move-local-there");
    let content = write_article(&here, "");
    let assets = article::assets(&content);
    fs::create_dir_all(&assets).unwrap();
    fs::write(assets.join("media-cd.png"), "bytes").unwrap();
    fs::write(
        &content,
        format!("![shot]({}/media-cd.png)\n", assets.display()),
    )
    .unwrap();

    let arrived = article::move_to(&content, there.path()).unwrap();

    let landed = article::assets(&arrived);
    assert_eq!(fs::read(landed.join("media-cd.png")).unwrap(), b"bytes");
    let text = fs::read_to_string(&arrived).unwrap();
    assert!(text.contains(&landed.display().to_string()), "{text}");
    assert!(!text.contains(&assets.display().to_string()), "{text}");
}
