mod common;

use common::{Scratch, invalid, refused, said};
use cydonia_mcp::{Server, tools};
use serde_json::{Value, json};
use std::sync::{Arc, atomic::AtomicBool};

const ORIGINAL: &str =
    "# Notes\n\n## Architecture\n\nStatus: draft\n\n## Design\n\nStatus: draft\n";

fn fixture(name: &str) -> (Scratch, Server) {
    let scratch = Scratch::new(name);
    let server = scratch.server();
    said(server.call(
        "article_add",
        json!({ "title": "Notes", "text": ORIGINAL }),
        Some(scratch.path()),
    ));
    (scratch, server)
}

fn read(server: &Server, scratch: &Scratch) -> String {
    said(server.call(
        "article_read",
        json!({ "article": "Notes" }),
        Some(scratch.path()),
    ))
}

#[test]
fn context_targets_one_passage_and_preserves_the_title() {
    let (scratch, server) = fixture("edit-context");
    said(server.call(
        "article_edit",
        json!({
            "project": scratch.path(),
            "article": "Notes",
            "old_string": "## Architecture\n\nStatus: draft",
            "new_string": "## Architecture\n\nStatus: approved ✅",
        }),
        None,
    ));
    assert_eq!(
        read(&server, &scratch),
        ORIGINAL.replacen("draft", "approved ✅", 1)
    );
}

#[test]
fn ambiguous_or_missing_matches_do_not_change_the_article() {
    let (scratch, server) = fixture("edit-refused");
    for (old, all, message) in [
        ("draft", false, "2 occurrences"),
        ("Draft", false, "not found"),
        ("Status:  draft", true, "not found"),
    ] {
        let why = refused(server.call(
            "article_edit",
            json!({ "article": "Notes", "old_string": old, "new_string": "changed", "replace_all": all }),
            Some(scratch.path()),
        ));
        assert!(why.contains(message), "{why}");
        assert_eq!(read(&server, &scratch), ORIGINAL);
    }
    let why = refused(server.call(
        "article_edit",
        json!({ "article": "Notes", "old_string": "draft", "new_string": "changed" }),
        Some(scratch.path()),
    ));
    assert!(why.contains("2 occurrences"));
    assert_eq!(read(&server, &scratch), ORIGINAL);
}

#[test]
fn replace_all_is_explicit_and_does_not_reprocess_inserted_text() {
    let (scratch, server) = fixture("edit-all");
    let answer = match server.call(
        "article_edit",
        json!({ "article": "Notes", "old_string": "draft", "new_string": "draft revised", "replace_all": true }),
        Some(scratch.path()),
    ) {
        Ok(answer) => answer,
        Err(_) => panic!("edit should succeed"),
    };
    assert_eq!(answer.data.unwrap()["replacements"], 2);
    assert_eq!(
        read(&server, &scratch),
        ORIGINAL.replace("draft", "draft revised")
    );
}

#[test]
fn edits_can_insert_and_delete_literal_markdown() {
    let (scratch, server) = fixture("edit-insert-delete");
    let image = "# Notes\n\n![图](media.png)";
    for (old, new, expected) in [
        ("# Notes", image, ORIGINAL.replacen("# Notes", image, 1)),
        ("\n\n![图](media.png)", "", ORIGINAL.to_owned()),
    ] {
        said(server.call(
            "article_edit",
            json!({ "article": "Notes", "old_string": old, "new_string": new }),
            Some(scratch.path()),
        ));
        assert_eq!(read(&server, &scratch), expected);
    }
}

#[test]
fn invalid_arguments_leave_the_article_unchanged() {
    let (scratch, server) = fixture("edit-invalid");
    for arguments in [
        json!({ "article": "Notes", "old_string": "", "new_string": "x" }),
        json!({ "article": "Notes", "old_string": "draft", "new_string": "x", "replace_all": "true" }),
        json!({ "article": "Notes", "old_string": "draft", "new_string": "x", "replace_all": null }),
        json!({ "article": "Notes", "old_string": "draft" }),
        json!({ "article": "Notes", "new_string": "x" }),
    ] {
        invalid(server.call("article_edit", arguments, Some(scratch.path())));
        assert_eq!(read(&server, &scratch), ORIGINAL);
    }
}

#[test]
fn edit_schema_has_an_optional_boolean_and_respects_project_binding() {
    let tool = tools::article::TOOLS
        .iter()
        .find(|tool| tool.name == "article_edit")
        .unwrap();
    for bound in [false, true] {
        let schema = (tool.schema)(bound);
        assert_eq!(schema["properties"]["replace_all"]["type"], "boolean");
        assert_eq!(schema["properties"]["replace_all"]["default"], false);
        let required = schema["required"].as_array().unwrap();
        assert!(!required.contains(&json!("replace_all")));
        for name in ["article", "old_string", "new_string"] {
            assert!(required.contains(&json!(name)));
        }
        // Offered either way; required only where there is no binding to
        // stand in for it, so a bound session can still name another project.
        assert_eq!(required.contains(&json!("project")), !bound);
        assert!(schema["properties"].get("project").is_some());
    }
}

#[test]
fn read_only_mode_refuses_edits() {
    let (scratch, server) = fixture("edit-read-only");
    let server = server.writable(Arc::new(AtomicBool::new(false)));
    let why = refused(server.call("article_edit", Value::Null, Some(scratch.path())));
    assert!(why.contains("read only"));
    assert_eq!(read(&server, &scratch), ORIGINAL);
}
