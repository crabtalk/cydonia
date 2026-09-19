mod common;

use common::{Rail, Scratch, refused, said};
use serde_json::json;

#[test]
fn external_and_bound_agents_share_references_across_renames() {
    let scratch = Scratch::new("entry-agents");
    let server = scratch.server();
    let board = scratch.store_create("Roadmap", "ROAD").unwrap();
    let added = server
        .call(
            "article_add",
            json!({"project": scratch.path(), "title": "Draft", "text": "hello"}),
            None,
        )
        .unwrap_or_else(|_| panic!("tool call failed"));
    let number = added.data.unwrap()["number"].as_u64().unwrap();
    assert_ne!(Some(number), board.number);
    let reference = format!("#{number}");
    assert_eq!(
        said(server.call(
            "article_read",
            json!({"article": reference}),
            Some(scratch.path())
        )),
        "hello"
    );
    said(server.call(
        "article_rename",
        json!({"article": reference, "title": "Final"}),
        Some(scratch.path()),
    ));
    let catalog = server
        .call("project_entries", json!({"project": scratch.path()}), None)
        .unwrap_or_else(|_| panic!("tool call failed"));
    assert!(
        catalog
            .text
            .contains(&format!("{reference} [article] Final"))
    );
    assert_eq!(
        catalog.data.unwrap()["entries"].as_array().unwrap().len(),
        2
    );
    assert_eq!(
        said(scratch.server().call(
            "project_read_entry",
            json!({"entry": reference}),
            Some(scratch.path())
        )),
        "hello"
    );
    let board_ref = format!("#{}", board.number.unwrap());
    assert!(
        said(server.call(
            "board_read",
            json!({"board": board_ref}),
            Some(scratch.path())
        ))
        .contains("Roadmap")
    );
    assert!(
        refused(server.call(
            "article_rewrite",
            json!({"article": board_ref, "text": "wrong"}),
            Some(scratch.path())
        ))
        .contains("no article")
    );
    assert!(
        refused(server.call(
            "project_read_entry",
            json!({"entry": "#9999"}),
            Some(scratch.path())
        ))
        .contains("no entry")
    );
}

/// A number means something only next to the project it was issued in, and
/// which project a call is about is the call's to say: the binding where it
/// names none, and the one it names where it does.
#[test]
fn references_are_scoped_to_the_project_a_call_is_about() {
    let one = Scratch::new("entry-scope-one");
    let two = Scratch::new("entry-scope-two");
    let server = one.server();
    Rail::also(two.path());
    for (root, text) in [(one.path(), "one"), (two.path(), "two")] {
        said(server.call(
            "article_add",
            json!({"title": "Notes", "text": text}),
            Some(root),
        ));
    }
    assert_eq!(
        said(server.call(
            "project_read_entry",
            json!({"entry": "#1"}),
            Some(one.path())
        )),
        "one"
    );
    assert_eq!(
        said(server.call(
            "project_read_entry",
            json!({"project": two.path(), "entry": "#1"}),
            Some(one.path())
        )),
        "two"
    );
}

/// The whole of the thing this session was about: a session in one project
/// writes an article into another, and the article lands there and not here.
#[test]
fn a_bound_session_writes_into_another_open_project() {
    let here = Scratch::new("cross-here");
    let there = Scratch::new("cross-there");
    let server = here.server();
    Rail::also(there.path());

    said(server.call(
        "article_add",
        json!({"project": there.path(), "title": "What A knows about B", "text": "findings"}),
        Some(here.path()),
    ));

    assert_eq!(
        said(server.call(
            "article_read",
            json!({"article": "What A knows about B"}),
            Some(there.path())
        )),
        "findings"
    );
    assert!(
        said(server.call("article_list", json!({}), Some(here.path()))).contains("no articles")
    );
}

/// A project cydonia does not have open is not one a call can name, however
/// real the directory is.
#[test]
fn a_project_that_is_not_open_cannot_be_named() {
    let here = Scratch::new("cross-closed");
    let elsewhere = Scratch::new("cross-elsewhere");
    let server = here.server();

    let why = refused(server.call(
        "article_add",
        json!({"project": elsewhere.path(), "title": "Notes", "text": "x"}),
        Some(here.path()),
    ));

    assert!(why.contains("does not have"), "{why}");
    assert!(!elsewhere.path().join(".cydonia").exists());
}
