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
    store.save_session(&filed).unwrap();
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
    cydonia_mcp::rail::set_agents(vec![
        cydonia_mcp::rail::Agent {
            name: "Claude Agent".to_owned(),
            id: Some("claude-acp".to_owned()),
        },
        cydonia_mcp::rail::Agent {
            name: "Codex".to_owned(),
            id: None,
        },
    ]);

    said(server.call(
        "session_send",
        json!({"agent": "claude agent", "message": "foo"}),
        Some(scratch.path()),
    ));

    assert!(rail.was_asked(Change::Start {
        project: scratch.path().canonicalize().unwrap(),
        agent: "claude-acp".to_owned(),
        message: "foo".to_owned(),
    }));
    let why = refused(server.call(
        "session_send",
        json!({"agent": "Claude", "message": "foo"}),
        Some(scratch.path()),
    ));
    assert!(
        why.contains("cydonia has Claude Agent (claude-acp), Codex"),
        "{why}"
    );
    let listing = server
        .handle(
            &serde_json::from_value(json!({"jsonrpc": "2.0", "id": 1, "method": "tools/list"}))
                .unwrap(),
            Some(scratch.path()),
        )
        .unwrap();
    let listing = serde_json::to_value(listing).unwrap();
    let send = listing["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .find(|tool| tool["name"] == "session_send")
        .unwrap()
        .clone();
    assert_eq!(
        send["inputSchema"]["properties"]["agent"]["enum"],
        json!(["claude-acp", "Codex"])
    );
}

/// A session filed in `scratch` holding `said`, a question and its answer
/// per pair. Answers its reference.
fn filed(scratch: &Scratch, said: &[(&str, &str)]) -> String {
    use artifact::project::Project as _;
    let store = scratch.store();
    let record = store.create_session().unwrap();
    let items: Vec<_> = said
        .iter()
        .flat_map(|(asked, answered)| [json!({ "User": asked }), json!({ "Agent": answered })])
        .collect();
    let filed: artifact::session::record::Record = serde_json::from_value(json!({
        "id": record, "agent": "claude", "title": "Talk", "name": null,
        "updated": 0, "items": items,
    }))
    .unwrap();
    store.save_session(&filed).unwrap();
    format!(
        "#{}",
        artifact::entry::number(scratch.path(), "session", &record).unwrap()
    )
}

#[test]
fn a_run_of_turns_is_read_by_reference() {
    let scratch = Scratch::new("session-read");
    let server = scratch.server();
    let at = filed(
        &scratch,
        &[("one", "a"), ("two", "b"), ("three", "c"), ("four", "d")],
    );

    let read = said(server.call(
        "session_read",
        json!({ "session": format!("{at}:2-3") }),
        Some(scratch.path()),
    ));

    assert!(read.contains(&format!("## {at}:2")), "{read}");
    assert!(
        read.contains("user: two") && read.contains("agent: c"),
        "{read}"
    );
    assert!(
        !read.contains("user: one") && !read.contains("user: four"),
        "{read}"
    );
}

#[test]
fn without_turns_the_latest_are_read() {
    let scratch = Scratch::new("session-read-latest");
    let server = scratch.server();
    let at = filed(
        &scratch,
        &[("one", "a"), ("two", "b"), ("three", "c"), ("four", "d")],
    );

    let read = said(server.call(
        "session_read",
        json!({ "session": at }),
        Some(scratch.path()),
    ));

    assert!(read.contains("turns 2-4 of 4"), "{read}");
    assert!(!read.contains("user: one"), "{read}");
}

#[test]
fn a_turn_past_the_end_is_refused() {
    let scratch = Scratch::new("session-read-past");
    let server = scratch.server();
    let at = filed(&scratch, &[("one", "a")]);

    let why = refused(server.call(
        "session_read",
        json!({ "session": at, "turns": "3" }),
        Some(scratch.path()),
    ));

    assert!(why.contains("has 1 turns"), "{why}");
}

#[test]
fn a_search_answers_turn_references() {
    let scratch = Scratch::new("session-search");
    let server = scratch.server();
    let first = filed(
        &scratch,
        &[("where is the parser", "in artifact"), ("thanks", "ok")],
    );
    let second = filed(&scratch, &[("hello", "hi"), ("PARSER again", "yes")]);

    let hits = said(server.call(
        "session_search",
        json!({ "query": "parser" }),
        Some(scratch.path()),
    ));

    assert!(
        hits.contains(&format!("{first}:1 Talk — where is the parser")),
        "{hits}"
    );
    assert!(
        hits.contains(&format!("{second}:2 Talk — PARSER again")),
        "{hits}"
    );
    assert!(!hits.contains(&format!("{first}:2")), "{hits}");

    let one = said(server.call(
        "session_search",
        json!({ "query": "parser", "session": second }),
        Some(scratch.path()),
    ));
    assert!(!one.contains(&first), "{one}");
}

/// A query is found however the file had to escape it.
#[test]
fn a_query_with_quotes_is_found() {
    let scratch = Scratch::new("session-search-quotes");
    let server = scratch.server();
    let at = filed(&scratch, &[(r#"say "hi" to C:\temp"#, "ok")]);

    let hits = said(server.call(
        "session_search",
        json!({ "query": r#""HI" to c:\"# }),
        Some(scratch.path()),
    ));

    assert!(hits.contains(&format!("{at}:1")), "{hits}");
}

/// A message from a session says which turn of it sent it.
#[test]
fn a_message_from_a_session_is_signed_with_its_turn() {
    let scratch = Scratch::new("session-send-signed");
    let server = scratch.server();
    let rail = Rail;
    let from = filed(&scratch, &[("one", "a"), ("two", "b")]);
    let to = filed(&scratch, &[("hello", "hi")]);
    let caller = artifact::entry::Registry::open(scratch.path())
        .unwrap()
        .resolve("session", from[1..].parse().unwrap())
        .unwrap()
        .unwrap();
    let target = artifact::entry::Registry::open(scratch.path())
        .unwrap()
        .resolve("session", to[1..].parse().unwrap())
        .unwrap()
        .unwrap();

    said(server.call_from(
        "session_send",
        json!({ "session": to, "message": "foo" }),
        Some(scratch.path()),
        Some(&caller),
    ));

    assert!(rail.was_asked(Change::Send {
        session: target,
        message: format!("from {from}:2\n\nfoo"),
    }));
}
