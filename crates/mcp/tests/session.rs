//! Sending a prompt to another session through the rail.

mod common;

use artifact::{kind::Kind, session::mode::Mode};
use common::{Rail, Scratch, refused, said, structured};
use cydonia_mcp::rail::{self, Change, Request, Settled, Started};
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
    let number = artifact::entry::number(scratch.path(), Kind::Session, &record).unwrap();

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
    cydonia_mcp::rail::set_agents(vec![
        cydonia_mcp::rail::Agent {
            name: "Reviewer".to_owned(),
            id: Some("reviewer-id".to_owned()),
            workspace: Some(scratch.path().canonicalize().unwrap()),
            mode: Default::default(),
        },
        cydonia_mcp::rail::Agent {
            name: "Claude Agent".to_owned(),
            id: Some("claude-acp".to_owned()),
            workspace: None,
            mode: Default::default(),
        },
        cydonia_mcp::rail::Agent {
            name: "Codex".to_owned(),
            id: None,
            workspace: None,
            mode: Default::default(),
        },
    ]);

    let project = scratch.path().canonicalize().unwrap();
    rail::install(move |request| {
        let Request::Start(start) = request else {
            panic!("expected a session start");
        };
        assert_eq!(start.project, project);
        assert_eq!(start.agent, "claude-acp");
        assert_eq!(start.message, "foo");
        start
            .reply
            .send(Ok(Started {
                id: "new-record".to_owned(),
                number: Some(17),
            }))
            .unwrap();
    });
    let call = serde_json::from_value(json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": {
            "name": "session_send",
            "arguments": {"agent": "claude agent", "message": "foo"}
        }
    }))
    .unwrap();
    let answer = server.handle(&call, Some(scratch.path())).unwrap();
    let answer = serde_json::to_value(answer).unwrap();
    assert_eq!(
        answer["result"]["content"][0]["text"],
        "started session #17 (ID new-record) on Claude Agent"
    );
    let why = refused(server.call(
        "session_send",
        json!({"agent": "Claude", "message": "foo"}),
        Some(scratch.path()),
    ));
    assert!(why.contains("no agent named Claude"), "{why}");
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
    let agent = &send["inputSchema"]["properties"]["agent"];
    assert_eq!(agent["enum"], json!(["reviewer-id", "claude-acp", "Codex"]));
    let choices = agent["description"].as_str().unwrap();
    assert!(choices.contains("Reviewer (reviewer-id, agent identity)"));
    assert!(choices.contains("Claude Agent (claude-acp, installed program)"));
    assert!(choices.contains("Codex (installed program)"));
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
        artifact::entry::number(scratch.path(), Kind::Session, &record).unwrap()
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

/// A message from a session is signed with a chip of the turn that sent it.
#[test]
fn a_message_from_a_session_is_signed_with_its_turn() {
    let scratch = Scratch::new("session-send-signed");
    let server = scratch.server();
    let rail = Rail;
    let from = filed(&scratch, &[("one", "a"), ("two", "b")]);
    let to = filed(&scratch, &[("hello", "hi")]);
    let caller = artifact::entry::Registry::open(scratch.path())
        .unwrap()
        .resolve(Kind::Session, from[1..].parse().unwrap())
        .unwrap()
        .unwrap();
    let target = artifact::entry::Registry::open(scratch.path())
        .unwrap()
        .resolve(Kind::Session, to[1..].parse().unwrap())
        .unwrap()
        .unwrap();

    said(server.call_from(
        "session_send",
        json!({ "session": to, "message": "foo" }),
        Some(scratch.path()),
        Some(&caller),
    ));

    let project = scratch.path().file_name().unwrap().to_string_lossy();
    let link = format!("cydonia://{project}{from}:2");
    assert!(rail.was_asked(Change::Send {
        session: target,
        message: format!(
            "from [{link}]({link} \"chip\"), which can wait on this turn and read your last \
             message in it as your answer\n\nfoo"
        ),
    }));
}

/// The record id a session reference such as `#4` names in `scratch`.
fn record_of(scratch: &Scratch, reference: &str) -> String {
    artifact::entry::Registry::open(scratch.path())
        .unwrap()
        .resolve(Kind::Session, reference[1..].parse().unwrap())
        .unwrap()
        .unwrap()
}

#[test]
fn a_wait_answers_the_last_message_of_the_turn_the_session_settled_on() {
    let scratch = Scratch::new("session-wait");
    let server = scratch.server();
    let to = filed(&scratch, &[("one", "a"), ("two", "b")]);
    let record = record_of(&scratch, &to);
    rail::install(move |request| {
        let Request::Wait(wait) = request else {
            panic!("expected a wait");
        };
        assert_eq!(wait.session, record);
        wait.reply
            .send(Ok(Settled {
                turn: 2,
                answer: Some("done".to_owned()),
                notice: None,
            }))
            .unwrap();
    });

    let answer = said(server.call(
        "session_wait",
        json!({ "session": to }),
        Some(scratch.path()),
    ));

    assert_eq!(answer, format!("{to}:2 answered:\n\ndone"));
}

#[test]
fn a_wait_past_its_timeout_says_the_session_is_still_working() {
    static HELD: std::sync::Mutex<Vec<rail::Wait>> = std::sync::Mutex::new(Vec::new());
    let scratch = Scratch::new("session-wait-timeout");
    let server = scratch.server();
    let to = filed(&scratch, &[("one", "a")]);
    rail::install(|request| {
        if let Request::Wait(wait) = request {
            HELD.lock().unwrap().push(wait);
        }
    });

    let answer = said(server.call(
        "session_wait",
        json!({ "session": to, "timeout": 1 }),
        Some(scratch.path()),
    ));

    assert!(answer.contains("still working"), "{answer}");
}

#[test]
fn a_session_cannot_wait_on_itself() {
    let scratch = Scratch::new("session-wait-self");
    let server = scratch.server();
    let to = filed(&scratch, &[("one", "a")]);
    let record = record_of(&scratch, &to);

    let why = refused(server.call_from(
        "session_wait",
        json!({ "session": to }),
        Some(scratch.path()),
        Some(&record),
    ));

    assert!(why.contains("itself"), "{why}");
}

/// A session filed in `scratch` titled `title`, and named `name` by the user
/// where one is given. Answers its record id.
fn titled(scratch: &Scratch, title: &str, name: Option<&str>) -> String {
    use artifact::project::Project as _;
    let store = scratch.store();
    let record = store.create_session().unwrap();
    let filed: artifact::session::record::Record = serde_json::from_value(json!({
        "id": record, "agent": "claude", "title": title, "name": name,
        "updated": 0, "items": [],
    }))
    .unwrap();
    store.save_session(&filed).unwrap();
    record
}

#[test]
fn a_session_renames_itself() {
    let scratch = Scratch::new("session-rename");
    let server = scratch.server();
    let rail = Rail;
    let record = titled(&scratch, "Old", None);

    said(server.call_from(
        "session_rename",
        json!({ "title": " New " }),
        Some(scratch.path()),
        Some(&record),
    ));

    assert!(rail.was_asked(Change::Rename {
        session: record,
        title: "New".to_owned(),
    }));
}

#[test]
fn a_title_the_user_gave_is_replaced_only_when_asked() {
    let scratch = Scratch::new("session-rename-named");
    let server = scratch.server();
    let rail = Rail;
    let record = titled(&scratch, "Old", Some("Mine"));

    let why = refused(server.call_from(
        "session_rename",
        json!({ "title": "New" }),
        Some(scratch.path()),
        Some(&record),
    ));
    assert!(why.contains("\"Mine\""), "{why}");

    said(server.call_from(
        "session_rename",
        json!({ "title": "New", "replace_user_title": true }),
        Some(scratch.path()),
        Some(&record),
    ));
    assert!(rail.was_asked(Change::Rename {
        session: record,
        title: "New".to_owned(),
    }));
}

#[test]
fn a_caller_that_is_not_a_session_cannot_rename() {
    let scratch = Scratch::new("session-rename-nobody");
    let server = scratch.server();

    let why = refused(server.call(
        "session_rename",
        json!({ "title": "New" }),
        Some(scratch.path()),
    ));

    assert!(why.contains("not one"), "{why}");
}

#[test]
fn a_terminal_session_reports_the_transcript_boundary() {
    use artifact::{project::Project as _, session::record::Record};
    let scratch = Scratch::new("terminal-read");
    let store = scratch.store();
    let id = store.create_session().unwrap();
    let record: Record = serde_json::from_value(json!({
        "id": id, "agent": "Claude", "mode": "terminal", "title": "Native CLI",
        "name": null, "updated": 0, "items": []
    }))
    .unwrap();
    store.save_session(&record).unwrap();
    let number = artifact::entry::number(scratch.path(), Kind::Session, &id).unwrap();
    let text = said(scratch.server().call(
        "session_read",
        json!({"session": format!("#{number}")}),
        Some(scratch.path()),
    ));
    assert!(text.contains("native terminal session"), "{text}");
    assert!(text.contains("structured turns are not recorded"), "{text}");
}

#[test]
fn a_named_terminal_agent_requests_a_run_with_the_initial_prompt_in_its_workspace() {
    let scratch = Scratch::new("native-agent-start");
    let other = Scratch::new("native-agent-other");
    let server = scratch.server();
    cydonia_mcp::rail::set_agents(vec![cydonia_mcp::rail::Agent {
        name: "Native Claude".into(),
        id: Some("named-agent-id".into()),
        workspace: Some(scratch.path().into()),
        mode: Mode::Terminal,
    }]);
    let project = scratch.path().canonicalize().unwrap();
    rail::install(move |request| {
        let Request::Start(start) = request else {
            panic!("expected a terminal run");
        };
        assert_eq!(start.project, project);
        assert_eq!(start.agent, "named-agent-id");
        assert_eq!(start.message, "fix it");
        start
            .reply
            .send(Ok(Started {
                id: "terminal-2".to_owned(),
                number: None,
            }))
            .unwrap();
    });
    let response = structured(server.call(
        "session_send",
        json!({"agent": "named-agent-id", "message": "fix it"}),
        Some(scratch.path()),
    ));
    assert_eq!(response, json!({"id": "terminal-2", "reference": null}));
    other.server();
    let why = refused(server.call(
        "session_send",
        json!({"agent": "named-agent-id", "message": "wrong project"}),
        Some(other.path()),
    ));
    assert!(why.contains("no agent named"), "{why}");
}

#[test]
fn messages_to_old_terminal_records_are_refused_without_claiming_delivery() {
    use artifact::{project::Project as _, session::record::Record};
    let scratch = Scratch::new("native-agent-prompt");
    let server = scratch.server();
    let rail = Rail;
    let store = scratch.store();
    let id = store.create_session().unwrap();
    let record: Record = serde_json::from_value(json!({ "id": id, "agent": "Claude", "mode": "terminal", "title": "Native", "name": null, "updated": 0, "items": [] })).unwrap();
    store.save_session(&record).unwrap();
    let number = artifact::entry::number(scratch.path(), Kind::Session, &id).unwrap();
    let response = refused(server.call(
        "session_send",
        json!({"session": format!("#{number}"), "message": "fix it"}),
        Some(scratch.path()),
    ));
    assert!(response.contains("native CLI"), "{response}");
    assert!(!rail.was_asked(Change::Send {
        session: id,
        message: "fix it".into()
    }));
}
