//! Sending a prompt to another session through the rail.

mod common;

use common::{Rail, Scratch, refused, said};
use cydonia_mcp::rail::Change;
use serde_json::json;

#[test]
fn a_session_is_sent_to_by_its_reference() {
    use artifact::project::Project as _;
    let scratch = Scratch::new("session-send");
    let server = scratch.server();
    let rail = Rail;
    let store = scratch.store();
    let record = store.create_session().unwrap();
    let filed: artifact::session::record::Record =
        serde_json::from_value(json!({"id": record, "agent": "claude", "title": "t", "name": null, "updated": 0, "items": []})).unwrap();
    store.save_session(&filed);
    let number = artifact::entry::number(scratch.path(), "session", &record).unwrap();

    said(server.call(
        "session_send",
        json!({"session": format!("#{number}"), "message": "foo"}),
        Some(scratch.path()),
    ));

    assert!(rail.was_asked(Change::Send {
        session: record,
        message: "foo".to_owned(),
    }));
}

#[test]
fn an_entry_that_is_not_a_session_is_refused() {
    let scratch = Scratch::new("session-send-board");
    let server = scratch.server();
    let board = scratch.store_create("Roadmap", "ROAD").unwrap();

    let why = refused(server.call(
        "session_send",
        json!({"session": format!("#{}", board.number.unwrap()), "message": "foo"}),
        Some(scratch.path()),
    ));

    assert!(why.contains("not a session"), "{why}");
}

#[test]
fn no_session_starts_one_on_the_named_agent() {
    let scratch = Scratch::new("session-start");
    let server = scratch.server();
    let rail = Rail;

    said(server.call(
        "session_send",
        json!({"agent": "Claude", "message": "foo"}),
        Some(scratch.path()),
    ));

    assert!(rail.was_asked(Change::Start {
        project: scratch.path().canonicalize().unwrap(),
        agent: "Claude".to_owned(),
        message: "foo".to_owned(),
    }));
}
