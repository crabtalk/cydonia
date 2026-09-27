mod common;

use common::Scratch;
use cydonia_artifact::{
    board::Board,
    project::Project as _,
    search::{self, Block, Kind, Match, Query},
    session::record::Record,
};
use std::sync::mpsc;

fn collect(each: impl FnOnce(&mpsc::Sender<Match>)) -> Vec<Match> {
    let (tx, rx) = mpsc::channel();
    each(&tx);
    drop(tx);
    let mut hits: Vec<Match> = rx.into_iter().collect();
    hits.sort_by_key(|hit| {
        (
            hit.item.id.clone(),
            format!("{:?}", hit.block),
            hit.range.start,
        )
    });
    hits
}

fn record() -> Record {
    serde_json::from_value(serde_json::json!({
        "id": "s1", "agent": "claude", "title": "Chat", "name": null, "updated": 1,
        "items": [
            {"User": "where is the \"Pump\"?"},
            {"Thinking": {"text": "pump pump", "done": true}},
            {"Agent": "the pump\nruns per chunk"}
        ]
    }))
    .unwrap()
}

#[test]
fn a_blank_query_is_no_query() {
    assert!(Query::literal("  ").is_none());
}

#[test]
fn a_pattern_matches_as_a_regex_ignoring_case() {
    let query = Query::pattern(r"P\w+P").unwrap().unwrap();
    let ranges: Vec<_> = query.find("the pump, a pulp").collect();
    assert_eq!(ranges, [4..8, 12..16]);
}

#[test]
fn a_pattern_skips_empty_matches() {
    let query = Query::pattern("x*").unwrap().unwrap();
    let ranges: Vec<_> = query.find("axxb").collect();
    assert_eq!(ranges, [1..3]);
}

#[test]
fn a_broken_pattern_is_an_error_and_a_blank_one_none() {
    assert!(Query::pattern("(").is_err());
    assert!(Query::pattern(" ").unwrap().is_none());
}

#[test]
fn matches_carry_block_range_and_line_ignoring_case_and_thinking() {
    let query = Query::literal("PUMP").unwrap();
    let record = record();
    let hits = collect(|tx| search::run(&[&record], &query, tx));
    let blocks: Vec<&Block> = hits.iter().map(|hit| &hit.block).collect();
    assert_eq!(blocks, [&Block::Chat(0), &Block::Chat(2)]);
    assert_eq!(hits[1].range, 4..8);
    assert_eq!(hits[1].line, "the pump");
    assert_eq!(hits[1].column, 4..8);
}

#[test]
fn a_dropped_receiver_stops_the_search() {
    let query = Query::literal("pump").unwrap();
    let record = record();
    let (tx, rx) = mpsc::channel();
    drop(rx);
    assert!(!search::one(&record, &query, &tx));
}

#[test]
fn disk_finds_articles_boards_and_sessions_without_writing() {
    let scratch = Scratch::new("search-disk");
    let store = scratch.store();
    store
        .create_article("# Notes\nthe \"quoted\" word\n")
        .unwrap();
    let mut board = store.create_board("Work", "").unwrap();
    let todo = board.add_column("Todo").id.clone();
    board.add_card(&todo, "find the \"quoted\" card".into());
    store.save_board(&mut board).unwrap();
    let mut session = record();
    session.id = store.create_session().unwrap();
    session
        .items
        .push(cydonia_artifact::session::chat::ChatItem::User(
            "a \"quoted\" ask".into(),
        ));
    store.save_session(&session).unwrap();
    let before: Board = store.board(&board.id).unwrap();

    let query = Query::literal("\"quoted\"").unwrap();
    let hits = collect(|tx| {
        search::disk(
            &store,
            &[Kind::Article, Kind::Board, Kind::Session],
            &query,
            tx,
        )
    });
    let mut kinds: Vec<Kind> = hits.iter().map(|hit| hit.item.kind).collect();
    kinds.sort_by_key(|kind| kind.key());
    assert_eq!(kinds, [Kind::Article, Kind::Board, Kind::Session]);
    let article = hits
        .iter()
        .find(|hit| hit.item.kind == Kind::Article)
        .unwrap();
    assert_eq!(article.block, Block::Line(1));
    assert_eq!(store.board(&board.id).unwrap().version, before.version);
}
