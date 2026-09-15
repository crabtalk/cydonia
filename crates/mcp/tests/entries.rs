mod common;

use common::{Scratch, refused, said};
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

#[test]
fn references_are_scoped_to_the_bound_project() {
    let one = Scratch::new("entry-scope-one");
    let two = Scratch::new("entry-scope-two");
    let server = one.server();
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
            json!({"project": two.path(), "entry": "#1"}),
            Some(one.path())
        )),
        "one"
    );
}
