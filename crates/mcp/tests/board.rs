//! A board is worked through the names a person would say, and what a tool
//! writes is on the disk the app reads back.

mod common;

use artifact::project::Project as _;
use common::{Scratch, invalid, refused, said};
use cydonia_mcp::proto::Request;
use serde_json::json;

/// The three ways a board arrives in a sentence, all of them the same board.
#[test]
fn a_board_answers_to_key_name_and_id() {
    let scratch = Scratch::new("addressing");
    let board = scratch
        .store()
        .create_board("Roadmap", "ROAD")
        .expect("a board");
    let server = scratch.server();

    for needle in ["ROAD", "road", "Roadmap", board.id.as_str()] {
        let text = said(server.call(
            "board_read",
            json!({ "project": scratch.path(), "board": needle }),
        ));
        assert!(text.starts_with("Roadmap (ROAD)"), "{needle}: {text}");
    }
}

/// A handle names one card across the project, so nothing that takes a card
/// also has to be told which board it is on.
#[test]
fn a_card_is_addressed_by_the_handle_it_is_given() {
    let scratch = Scratch::new("handles");
    scratch
        .store()
        .create_board("Roadmap", "ROAD")
        .expect("a board");
    let server = scratch.server();
    said(server.call(
        "board_add_column",
        json!({ "project": scratch.path(), "board": "ROAD", "name": "Todo" }),
    ));
    said(server.call(
        "board_add_column",
        json!({ "project": scratch.path(), "board": "ROAD", "name": "Doing" }),
    ));

    let added = said(server.call(
        "board_add_card",
        json!({ "project": scratch.path(), "board": "ROAD", "column": "Todo", "text": "Wire the model picker" }),
    ));
    assert_eq!(added, "ROAD-1 added to Todo");

    // No board argument anywhere below: the handle carries it.
    let moved = said(server.call(
        "board_move_card",
        json!({ "project": scratch.path(), "card": "ROAD-1", "column": "Doing" }),
    ));
    assert_eq!(moved, "ROAD-1 moved to Doing");
    let text = said(server.call(
        "board_read",
        json!({ "project": scratch.path(), "board": "ROAD" }),
    ));
    assert!(
        text.contains("Doing\n  ROAD-1  Wire the model picker"),
        "{text}"
    );
}

/// `ROA2-5` is card 5 on the second Roadmap, not card `2-5` on `ROA` — the key
/// may carry a digit, so the split is the last dash.
#[test]
fn a_handle_splits_on_its_last_dash() {
    let scratch = Scratch::new("last-dash");
    let store = scratch.store();
    store.create_board("Roadmap", "ROA").expect("a board");
    store.create_board("Roadmap", "ROA2").expect("another");
    let server = scratch.server();
    said(server.call(
        "board_add_column",
        json!({ "project": scratch.path(), "board": "ROA2", "name": "Todo" }),
    ));
    said(server.call(
        "board_add_card",
        json!({ "project": scratch.path(), "board": "ROA2", "column": "Todo", "text": "the second one" }),
    ));

    let text = said(server.call(
        "board_rewrite_card",
        json!({ "project": scratch.path(), "card": "ROA2-1", "text": "still it" }),
    ));
    assert_eq!(text, "ROA2-1 now reads: still it");
}

/// A refusal names what is actually there. A model that guessed wrong fixes it
/// from the answer instead of spending a turn asking.
#[test]
fn a_refusal_says_what_is_there() {
    let scratch = Scratch::new("refusals");
    scratch
        .store()
        .create_board("Roadmap", "ROAD")
        .expect("a board");
    let server = scratch.server();
    said(server.call(
        "board_add_column",
        json!({ "project": scratch.path(), "board": "ROAD", "name": "Todo" }),
    ));

    let why = refused(server.call(
        "board_read",
        json!({ "project": scratch.path(), "board": "Backlog" }),
    ));
    assert!(why.contains("ROAD (Roadmap)"), "{why}");

    let why = refused(server.call(
        "board_add_card",
        json!({ "project": scratch.path(), "board": "ROAD", "column": "Doing", "text": "x" }),
    ));
    assert!(why.contains("Todo"), "{why}");

    let why = refused(server.call(
        "board_move_card",
        json!({ "project": scratch.path(), "card": "ROAD-9", "column": "Todo" }),
    ));
    assert!(why.contains("ROAD-9"), "{why}");
}

/// The board's own refusal, reached through a tool: a column is only where
/// work sits, so dropping one never means dropping the work.
#[test]
fn a_column_holding_cards_is_not_dropped() {
    let scratch = Scratch::new("columns");
    scratch
        .store()
        .create_board("Roadmap", "ROAD")
        .expect("a board");
    let server = scratch.server();
    said(server.call(
        "board_add_column",
        json!({ "project": scratch.path(), "board": "ROAD", "name": "Todo" }),
    ));
    said(server.call(
        "board_add_card",
        json!({ "project": scratch.path(), "board": "ROAD", "column": "Todo", "text": "Retire Spot" }),
    ));

    let why = refused(server.call(
        "board_remove_column",
        json!({ "project": scratch.path(), "board": "ROAD", "column": "Todo" }),
    ));
    assert!(why.contains("still holds cards"), "{why}");

    said(server.call(
        "board_remove_card",
        json!({ "project": scratch.path(), "card": "ROAD-1" }),
    ));
    let text = said(server.call(
        "board_remove_column",
        json!({ "project": scratch.path(), "board": "ROAD", "column": "Todo" }),
    ));
    assert_eq!(text, "Todo removed from Roadmap");
}

/// A missing argument is the client being wrong, not the model — so it leaves
/// as a protocol error rather than as something to try again.
#[test]
fn a_missing_argument_is_not_a_refusal() {
    let scratch = Scratch::new("arguments");
    scratch
        .store()
        .create_board("Roadmap", "ROAD")
        .expect("a board");
    let server = scratch.server();

    let why = invalid(server.call("board_read", json!({ "project": scratch.path() })));
    assert!(why.contains("board"), "{why}");
    let why = invalid(server.call("board_read", json!({ "board": "ROAD" })));
    assert!(why.contains("project"), "{why}");
    let why = invalid(server.call("no_such_tool", json!({})));
    assert!(why.contains("no_such_tool"), "{why}");
}

/// The app is the reader of everything written here, so a tool's work has to
/// be on disk the way a click's is.
#[test]
fn what_a_tool_wrote_is_on_disk() {
    let scratch = Scratch::new("on-disk");
    scratch
        .store()
        .create_board("Roadmap", "ROAD")
        .expect("a board");
    let server = scratch.server();
    said(server.call(
        "board_add_column",
        json!({ "project": scratch.path(), "board": "ROAD", "name": "Todo" }),
    ));
    said(server.call(
        "board_add_card",
        json!({ "project": scratch.path(), "board": "ROAD", "column": "Todo", "text": "Wire the model picker" }),
    ));

    // A fresh store, the way the watch re-reads one.
    let boards = scratch.store().boards();
    let card = boards
        .iter()
        .flat_map(|board| &board.columns)
        .flat_map(|column| &column.cards)
        .next()
        .expect("the card that was added");
    assert_eq!(card.text, "Wire the model picker");
    assert_eq!(card.handle, Some(1));
}

/// A frame with no id is a notification whatever its method is, and answering
/// one is a protocol error rather than a courtesy.
#[test]
fn a_notification_is_not_answered() {
    let scratch = Scratch::new("envelope");
    let server = scratch.server();

    let notification: Request =
        serde_json::from_value(json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }))
            .expect("a frame");
    assert!(server.handle(&notification).is_none());

    let call: Request =
        serde_json::from_value(json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/list" }))
            .expect("a frame");
    let answer = server.handle(&call).expect("an answer");
    let tools = answer.result.expect("a result");
    let names: Vec<&str> = tools["tools"]
        .as_array()
        .expect("a list")
        .iter()
        .filter_map(|tool| tool["name"].as_str())
        .collect();
    // Every mounted set is in one list, which is what an agent sees.
    assert!(names.contains(&"board_list"), "{names:?}");
    assert!(names.contains(&"article_list"), "{names:?}");
}

/// A call that was told no comes back as a *result* the model can read, not as
/// an error it can only give up on.
#[test]
fn a_refusal_reaches_the_model_as_a_result() {
    let scratch = Scratch::new("is-error");
    let server = scratch.server();

    let call: Request = serde_json::from_value(json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": {
            "name": "board_read",
            "arguments": { "project": scratch.path(), "board": "Nothing" },
        },
    }))
    .expect("a frame");
    let answer = server.handle(&call).expect("an answer");
    assert!(answer.error.is_none(), "a refusal is not a protocol error");
    let result = answer.result.expect("a result");
    assert_eq!(result["isError"], json!(true));
    assert!(
        result["content"][0]["text"]
            .as_str()
            .expect("the line")
            .contains("no board Nothing"),
    );
}

/// With editing off, a tool that changes a project is not in the list — and
/// calling it anyway is told why rather than told it does not exist.
#[test]
fn a_read_only_server_offers_no_way_to_write() {
    use std::sync::{Arc, atomic::AtomicBool};

    let scratch = Scratch::new("read-only");
    scratch.store_create("Roadmap", "ROAD").expect("a board");
    let switch = Arc::new(AtomicBool::new(false));
    let server = cydonia_mcp::Server::new()
        .mount(&cydonia_mcp::tools::board::TOOLS)
        .writable(switch.clone());

    let call: Request =
        serde_json::from_value(json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/list" }))
            .expect("a frame");
    let listed = server
        .handle(&call)
        .expect("an answer")
        .result
        .expect("a result");
    let names: Vec<&str> = listed["tools"]
        .as_array()
        .expect("a list")
        .iter()
        .filter_map(|tool| tool["name"].as_str())
        .collect();
    assert_eq!(names, ["board_list", "board_read"], "{names:?}");

    let why = refused(server.call(
        "board_add_column",
        json!({ "project": scratch.path(), "board": "ROAD", "name": "Todo" }),
    ));
    assert!(why.contains("read only"), "{why}");

    // And the same server writes again the moment the switch moves, with
    // nothing rebuilt — an agent is holding the URL.
    switch.store(true, std::sync::atomic::Ordering::Relaxed);
    said(server.call(
        "board_add_column",
        json!({ "project": scratch.path(), "board": "ROAD", "name": "Todo" }),
    ));
}
