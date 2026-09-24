//! A board is worked through the names a person would say, and what a tool
//! writes is on the disk the app reads back.

mod common;

use artifact::project::Project as _;
use common::{Rail, Scratch, invalid, refused, said};
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
            None,
        ));
        assert!(text.starts_with("#1 Roadmap (ROAD)"), "{needle}: {text}");
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
        None,
    ));
    said(server.call(
        "board_add_column",
        json!({ "project": scratch.path(), "board": "ROAD", "name": "Doing" }),
        None,
    ));

    let added = said(server.call(
        "board_add_card",
        json!({ "project": scratch.path(), "board": "ROAD", "column": "Todo", "text": "Wire the model picker" }),
    None));
    assert_eq!(added, "ROAD-1 added to TODO");

    // No board argument anywhere below: the handle carries it.
    let moved = said(server.call(
        "board_move_card",
        json!({ "project": scratch.path(), "card": "ROAD-1", "column": "Doing" }),
        None,
    ));
    assert_eq!(moved, "ROAD-1 moved to DOING");
    let text = said(server.call(
        "board_read",
        json!({ "project": scratch.path(), "board": "ROAD" }),
        None,
    ));
    assert!(
        text.contains("DOING\n  ROAD-1  Wire the model picker"),
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
        None,
    ));
    said(server.call(
        "board_add_card",
        json!({ "project": scratch.path(), "board": "ROA2", "column": "Todo", "text": "the second one" }),
    None));

    let text = said(server.call(
        "board_rewrite_card",
        json!({ "project": scratch.path(), "card": "ROA2-1", "text": "still it" }),
        None,
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
        None,
    ));

    let why = refused(server.call(
        "board_read",
        json!({ "project": scratch.path(), "board": "Backlog" }),
        None,
    ));
    assert!(why.contains("ROAD (Roadmap)"), "{why}");

    let why = refused(server.call(
        "board_add_card",
        json!({ "project": scratch.path(), "board": "ROAD", "column": "Doing", "text": "x" }),
        None,
    ));
    assert!(why.contains("TODO"), "{why}");

    let why = refused(server.call(
        "board_move_card",
        json!({ "project": scratch.path(), "card": "ROAD-9", "column": "Todo" }),
        None,
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
        None,
    ));
    said(server.call(
        "board_add_card",
        json!({ "project": scratch.path(), "board": "ROAD", "column": "Todo", "text": "Retire Spot" }),
    None));

    let why = refused(server.call(
        "board_remove_column",
        json!({ "project": scratch.path(), "board": "ROAD", "column": "Todo" }),
        None,
    ));
    assert!(why.contains("still holds cards"), "{why}");

    said(server.call(
        "board_remove_card",
        json!({ "project": scratch.path(), "card": "ROAD-1" }),
        None,
    ));
    let text = said(server.call(
        "board_remove_column",
        json!({ "project": scratch.path(), "board": "ROAD", "column": "Todo" }),
        None,
    ));
    assert_eq!(text, "TODO removed from Roadmap");
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

    let why = invalid(server.call("board_read", json!({ "project": scratch.path() }), None));
    assert!(why.contains("board"), "{why}");
    let why = invalid(server.call("board_read", json!({ "board": "ROAD" }), None));
    assert!(why.contains("project"), "{why}");
    let why = invalid(server.call("no_such_tool", json!({}), None));
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
        None,
    ));
    said(server.call(
        "board_add_card",
        json!({ "project": scratch.path(), "board": "ROAD", "column": "Todo", "text": "Wire the model picker" }),
    None));

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
    assert!(server.handle(&notification, None).is_none());

    let call: Request =
        serde_json::from_value(json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/list" }))
            .expect("a frame");
    let answer = server.handle(&call, None).expect("an answer");
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
    let answer = server.handle(&call, None).expect("an answer");
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
    Rail::also(scratch.path());
    scratch.store_create("Roadmap", "ROAD").expect("a board");
    let switch = Arc::new(AtomicBool::new(false));
    let server = cydonia_mcp::Server::new()
        .mount(&cydonia_mcp::tools::board::TOOLS)
        .writable(switch.clone());

    let call: Request =
        serde_json::from_value(json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/list" }))
            .expect("a frame");
    let listed = server
        .handle(&call, None)
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
        None,
    ));
    assert!(why.contains("read only"), "{why}");

    // And the same server writes again the moment the switch moves, with
    // nothing rebuilt — an agent is holding the URL.
    switch.store(true, std::sync::atomic::Ordering::Relaxed);
    said(server.call(
        "board_add_column",
        json!({ "project": scratch.path(), "board": "ROAD", "name": "Todo" }),
        None,
    ));
}

#[test]
fn agents_create_boards_with_normalized_unique_keys() {
    let scratch = Scratch::new("create-board");
    let server = scratch.server();
    let added = said(server.call(
        "board_add",
        json!({
            "project": scratch.path(), "name": " Roadmap ", "key": "road"
        }),
        None,
    ));
    assert!(added.starts_with("#1 Roadmap (ROAD)"));
    let boards = scratch.store().boards();
    assert_eq!(boards.len(), 1);
    assert_eq!(boards[0].name, "Roadmap");
    assert_eq!(boards[0].key, "ROAD");
    assert!(
        refused(server.call(
            "board_add",
            json!({
                "project": scratch.path(), "name": "Other", "key": "Road"
            }),
            None
        ))
        .contains("another board")
    );
    for (name, key) in [(" ", "NEW"), ("Other", "---")] {
        refused(server.call(
            "board_add",
            json!({
                "project": scratch.path(), "name": name, "key": key
            }),
            None,
        ));
    }
    assert_eq!(scratch.store().boards().len(), 1);
}

#[test]
fn board_creation_respects_bound_projects_and_read_only_mode() {
    use std::sync::{Arc, atomic::AtomicBool};
    let scratch = Scratch::new("create-bound-board");
    let server = scratch.server();
    said(server.call(
        "board_add",
        json!({"name": "Tasks", "key": "TASK"}),
        Some(scratch.path()),
    ));
    assert_eq!(scratch.store().boards()[0].key, "TASK");
    let server = scratch.server().writable(Arc::new(AtomicBool::new(false)));
    assert!(
        refused(server.call(
            "board_add",
            json!({"name": "Other", "key": "NEW"}),
            Some(scratch.path())
        ))
        .contains("read only")
    );
    assert_eq!(scratch.store().boards().len(), 1);
}

/// An agent says how the work on a card is going, and takes the word off when
/// the turn is over. The board reads back with the tag beside the card.
#[test]
fn a_card_is_tagged_and_untagged() {
    let scratch = Scratch::new("status");
    scratch
        .store()
        .create_board("Roadmap", "ROAD")
        .expect("a board");
    let server = scratch.server();
    said(server.call(
        "board_add_column",
        json!({ "project": scratch.path(), "board": "ROAD", "name": "Todo" }),
        None,
    ));
    said(server.call(
        "board_add_card",
        json!({ "project": scratch.path(), "board": "ROAD", "column": "Todo", "text": "Wire the model picker" }),
        None,
    ));

    let tagged = said(server.call(
        "board_set_card_status",
        json!({ "project": scratch.path(), "card": "ROAD-1", "status": "busy" }),
        None,
    ));
    assert_eq!(tagged, "ROAD-1 is busy");
    let text = said(server.call(
        "board_read",
        json!({ "project": scratch.path(), "board": "ROAD" }),
        None,
    ));
    assert!(
        text.contains("ROAD-1  Wire the model picker  [busy]"),
        "{text}"
    );

    let cleared = said(server.call(
        "board_set_card_status",
        json!({ "project": scratch.path(), "card": "ROAD-1", "status": "none" }),
        None,
    ));
    assert_eq!(cleared, "ROAD-1 is no longer tagged");
    let text = said(server.call(
        "board_read",
        json!({ "project": scratch.path(), "board": "ROAD" }),
        None,
    ));
    assert!(!text.contains("[busy]"), "{text}");
}

/// Several cards are tagged in one call, across boards — an agent taking a run
/// of work up should not spend a call per card.
#[test]
fn several_cards_are_tagged_in_one_call() {
    let scratch = Scratch::new("status-batch");
    let store = scratch.store();
    store.create_board("Roadmap", "ROAD").expect("a board");
    store.create_board("Development", "DEV").expect("a board");
    let server = scratch.server();
    for board in ["ROAD", "DEV"] {
        said(server.call(
            "board_add_column",
            json!({ "project": scratch.path(), "board": board, "name": "Todo" }),
            None,
        ));
    }
    for (board, text) in [
        ("ROAD", "Wire the model picker"),
        ("ROAD", "Ship the picker"),
        ("DEV", "Fix the drag"),
    ] {
        said(server.call(
            "board_add_card",
            json!({ "project": scratch.path(), "board": board, "column": "Todo", "text": text }),
            None,
        ));
    }

    let tagged = said(server.call(
        "board_set_card_status",
        json!({
            "project": scratch.path(),
            "card": ["ROAD-1", "ROAD-2", "DEV-1"],
            "status": "busy",
        }),
        None,
    ));
    assert_eq!(tagged, "ROAD-1, ROAD-2, DEV-1 are busy");
    // Every named card, not just one per board: two cards of one board are two
    // reads of it, and a write per card puts back a copy that predates the rest.
    for (board, tags) in [("ROAD", 2), ("DEV", 1)] {
        let text = said(server.call(
            "board_read",
            json!({ "project": scratch.path(), "board": board }),
            None,
        ));
        assert_eq!(text.matches("[busy]").count(), tags, "{text}");
    }

    // A card nobody can find refuses the whole call, so the list is never half
    // applied without saying so.
    refused(server.call(
        "board_set_card_status",
        json!({ "project": scratch.path(), "card": ["ROAD-1", "ROAD-9"], "status": "none" }),
        None,
    ));
    let text = said(server.call(
        "board_read",
        json!({ "project": scratch.path(), "board": "ROAD" }),
        None,
    ));
    assert!(text.contains("[busy]"), "and nothing was written: {text}");
}

/// A word nobody uses is refused with the ones that are.
#[test]
fn an_unknown_status_is_refused_with_the_list() {
    let scratch = Scratch::new("status-refused");
    scratch
        .store()
        .create_board("Roadmap", "ROAD")
        .expect("a board");
    let server = scratch.server();
    said(server.call(
        "board_add_column",
        json!({ "project": scratch.path(), "board": "ROAD", "name": "Todo" }),
        None,
    ));
    said(server.call(
        "board_add_card",
        json!({ "project": scratch.path(), "board": "ROAD", "column": "Todo", "text": "Wire it" }),
        None,
    ));

    let refusal = refused(server.call(
        "board_set_card_status",
        json!({ "project": scratch.path(), "card": "ROAD-1", "status": "working" }),
        None,
    ));
    assert_eq!(
        refusal,
        "working is not a status — say busy, blocked, done, none"
    );
}

/// A lane and the cards in it are laid out in two calls rather than five: the
/// list argument is what the board tools take wherever one thing pluralises
/// and the rest of the call stays one thing.
#[test]
fn a_board_is_laid_out_in_one_call_per_kind() {
    let scratch = Scratch::new("batch-layout");
    scratch
        .store()
        .create_board("Roadmap", "ROAD")
        .expect("a board");
    let server = scratch.server();

    let lanes = said(server.call(
        "board_add_column",
        json!({ "project": scratch.path(), "board": "ROAD", "name": ["Todo", "Doing", "Done"] }),
        None,
    ));
    assert_eq!(lanes, "TODO, DOING, DONE added to Roadmap");

    let cards = said(server.call(
        "board_add_card",
        json!({
            "project": scratch.path(),
            "board": "ROAD",
            "column": "Todo",
            "text": ["Retire Spot", "Feed Spot", "Walk Spot"],
        }),
        None,
    ));
    assert_eq!(cards, "ROAD-1, ROAD-2, ROAD-3 added to TODO");

    // In the order they were given, at the end of the lane they were put in.
    let read = said(server.call(
        "board_read",
        json!({ "project": scratch.path(), "board": "ROAD" }),
        None,
    ));
    let at = |needle: &str| read.find(needle).expect(needle);
    assert!(at("Retire Spot") < at("Feed Spot"), "{read}");
    assert!(at("Feed Spot") < at("Walk Spot"), "{read}");
}

/// Cards and columns go the same way they came. A column still holding one of
/// them refuses the whole call, so a board is never left half emptied of its
/// lanes.
#[test]
fn several_cards_and_columns_are_dropped_in_one_call() {
    let scratch = Scratch::new("batch-drop");
    scratch
        .store()
        .create_board("Roadmap", "ROAD")
        .expect("a board");
    let server = scratch.server();
    said(server.call(
        "board_add_column",
        json!({ "project": scratch.path(), "board": "ROAD", "name": ["Todo", "Doing", "Done"] }),
        None,
    ));
    said(server.call(
        "board_add_card",
        json!({
            "project": scratch.path(),
            "board": "ROAD",
            "column": "Todo",
            "text": ["Retire Spot", "Feed Spot"],
        }),
        None,
    ));

    // TODO still holds cards, so DOING and DONE stay too.
    let why = refused(server.call(
        "board_remove_column",
        json!({ "project": scratch.path(), "board": "ROAD", "column": ["Doing", "Todo", "Done"] }),
        None,
    ));
    assert!(why.contains("still holds cards"), "{why}");
    let read = said(server.call(
        "board_read",
        json!({ "project": scratch.path(), "board": "ROAD" }),
        None,
    ));
    assert!(read.contains("DOING"), "{read}");
    assert!(read.contains("DONE"), "{read}");

    let gone = said(server.call(
        "board_remove_card",
        json!({ "project": scratch.path(), "card": ["ROAD-1", "ROAD-2"] }),
        None,
    ));
    assert!(gone.contains("ROAD-1"), "{gone}");
    assert!(gone.contains("ROAD-2"), "{gone}");

    let dropped = said(server.call(
        "board_remove_column",
        json!({ "project": scratch.path(), "board": "ROAD", "column": ["Todo", "Doing", "Done"] }),
        None,
    ));
    assert_eq!(dropped, "TODO, DOING, DONE removed from Roadmap");
    let read = said(server.call(
        "board_read",
        json!({ "project": scratch.path(), "board": "ROAD" }),
        None,
    ));
    assert!(!read.contains("TODO"), "{read}");
}

/// A rename changes the name, and the key with it only when one is given.
#[test]
fn a_board_is_renamed_and_optionally_re_keyed() {
    let scratch = Scratch::new("rename-board");
    let board = scratch
        .store()
        .create_board("Roadmap", "ROAD")
        .expect("a board");
    let server = scratch.server();
    said(server.call(
        "board_add_column",
        json!({ "project": scratch.path(), "board": "ROAD", "name": "Todo" }),
        None,
    ));
    let handle = said(server.call(
        "board_add_card",
        json!({ "project": scratch.path(), "board": "ROAD", "column": "Todo", "text": "ship" }),
        None,
    ));
    assert!(handle.contains("ROAD-1"), "{handle}");

    // The name alone: the key, and so every handle, is left where it was.
    let text = said(server.call(
        "board_rename",
        json!({ "project": scratch.path(), "board": "ROAD", "name": " Plan " }),
        None,
    ));
    assert!(text.starts_with("Roadmap is now Plan"), "{text}");
    let boards = scratch.store().boards();
    assert_eq!(boards[0].name, "Plan");
    assert_eq!(boards[0].key, "ROAD", "the key is not touched by a rename");

    // And with a key, which is what every handle is read off.
    let text = said(server.call(
        "board_rename",
        json!({ "project": scratch.path(), "board": board.id.as_str(), "name": "Plan", "key": "back" }),
        None,
    ));
    assert!(text.contains("ROAD-1 is now BACK-1"), "{text}");
    assert_eq!(scratch.store().boards()[0].key, "BACK");
    let read = said(server.call(
        "board_read",
        json!({ "project": scratch.path(), "board": "BACK" }),
        None,
    ));
    assert!(read.contains("BACK-1"), "{read}");
}

/// A rename is refused whole: neither half lands when the key is one another
/// board here already has.
#[test]
fn a_rename_onto_a_taken_key_changes_nothing() {
    let scratch = Scratch::new("rename-clash");
    let store = scratch.store();
    store.create_board("Roadmap", "ROAD").expect("a board");
    store.create_board("Backlog", "BACK").expect("a board");
    let server = scratch.server();

    let why = refused(server.call(
        "board_rename",
        json!({ "project": scratch.path(), "board": "ROAD", "name": "Plan", "key": "back" }),
        None,
    ));
    assert!(why.contains("another board"), "{why}");
    let boards = scratch.store().boards();
    assert!(
        boards.iter().any(|board| board.name == "Roadmap"),
        "the name did not land either: {:?}",
        boards.iter().map(|board| &board.name).collect::<Vec<_>>()
    );

    // Its own key is not a clash with itself.
    said(server.call(
        "board_rename",
        json!({ "project": scratch.path(), "board": "ROAD", "name": "Plan", "key": "ROAD" }),
        None,
    ));
}

/// Deleting is its own door: a server that may write is not thereby a server
/// that may empty a project.
#[test]
fn deleting_is_withheld_until_its_own_switch_is_on() {
    use std::sync::{Arc, atomic::AtomicBool};

    let scratch = Scratch::new("delete-switch");
    Rail::also(scratch.path());
    scratch.store_create("Roadmap", "ROAD").expect("a board");
    let writable = Arc::new(AtomicBool::new(true));
    let deletes = Arc::new(AtomicBool::new(false));
    let server = cydonia_mcp::Server::new()
        .mount(&cydonia_mcp::tools::board::TOOLS)
        .writable(writable.clone())
        .deletes(deletes.clone());

    let listed = |server: &cydonia_mcp::Server| {
        let call: Request =
            serde_json::from_value(json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/list" }))
                .expect("a frame");
        server
            .handle(&call, None)
            .expect("an answer")
            .result
            .expect("a result")["tools"]
            .as_array()
            .expect("a list")
            .iter()
            .filter_map(|tool| tool["name"].as_str().map(str::to_owned))
            .collect::<Vec<_>>()
    };

    let names = listed(&server);
    assert!(
        names.iter().any(|name| name == "board_archive"),
        "putting away is ordinary editing: {names:?}"
    );
    assert!(
        !names.iter().any(|name| name == "board_remove"),
        "and deleting is not: {names:?}"
    );

    deletes.store(true, std::sync::atomic::Ordering::Relaxed);
    assert!(listed(&server).iter().any(|name| name == "board_remove"));

    // Writing off takes deleting with it, whatever the narrower switch says.
    writable.store(false, std::sync::atomic::Ordering::Relaxed);
    let names = listed(&server);
    assert!(
        !names.iter().any(|name| name == "board_remove"),
        "a server that may not change a project may not empty one: {names:?}"
    );
}

/// Archiving puts a board under the divider; deleting takes it off the disk.
#[test]
fn a_board_is_put_away_and_then_deleted() {
    let scratch = Scratch::new("archive-board");
    scratch.store_create("Roadmap", "ROAD").expect("a board");
    scratch.store_create("Backlog", "BACK").expect("a board");
    let server = scratch.server();

    let text = said(server.call(
        "board_archive",
        json!({ "project": scratch.path(), "board": "ROAD" }),
        None,
    ));
    assert!(text.contains("put away"), "{text}");
    let boards = scratch.store().boards();
    assert_eq!(boards.len(), 2, "archived is not gone");
    assert!(
        boards
            .iter()
            .any(|board| board.key == "ROAD" && board.archived),
        "and the board says so"
    );

    // Back again, which is the half a delete does not have.
    said(server.call(
        "board_archive",
        json!({ "project": scratch.path(), "board": "ROAD", "archived": false }),
        None,
    ));
    assert!(
        scratch
            .store()
            .boards()
            .iter()
            .any(|board| board.key == "ROAD" && !board.archived)
    );

    let text = said(server.call(
        "board_remove",
        json!({ "project": scratch.path(), "board": ["ROAD", "BACK"] }),
        None,
    ));
    assert!(text.contains("deleted"), "{text}");
    assert!(scratch.store().boards().is_empty(), "both are off the disk");
}

#[test]
fn busy_cards_record_the_calling_session_and_keep_it_when_cleared() {
    let scratch = Scratch::new("busy-session");
    let store = scratch.store();
    let mut board = store.create_board("Roadmap", "ROAD").unwrap();
    let column = board.add_column("Todo").id.clone();
    board.add_card(&column, "First".into());
    board.add_card(&column, "Second".into());
    store.save_board(&mut board).unwrap();
    let server = scratch.server();
    said(server.call_from(
        "board_set_card_status",
        json!({ "project": scratch.path(), "card": ["ROAD-1", "ROAD-2"], "status": "busy" }),
        None,
        Some("working-session"),
    ));
    for card in &store.boards()[0].columns[0].cards {
        assert_eq!(card.status, Some(artifact::board::Status::Busy));
        assert_eq!(card.session.as_deref(), Some("working-session"));
    }
    said(server.call_from(
        "board_set_card_status",
        json!({ "project": scratch.path(), "card": ["ROAD-1", "ROAD-2"], "status": "none" }),
        None,
        Some("other-session"),
    ));
    for card in &store.boards()[0].columns[0].cards {
        assert_eq!(card.status, None);
        assert_eq!(card.session.as_deref(), Some("working-session"));
    }
}
