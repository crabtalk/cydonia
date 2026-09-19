//! An article is written, found by the name it was given, and read back off
//! the disk the app reads.

mod common;

use common::{Scratch, invalid, refused, said};
use serde_json::json;

#[test]
fn article_results_expose_a_shared_absolute_media_path_without_creating_it() {
    let scratch = Scratch::new("article-assets");
    let server = scratch.server();
    let expected = std::fs::canonicalize(scratch.path())
        .unwrap()
        .join(".cydonia/assets");
    let made = server
        .call(
            "article_add",
            json!({ "project": scratch.path(), "title": "Pictures", "text": "Body" }),
            None,
        )
        .unwrap_or_else(|_| panic!("article creation failed"));
    let data = made.data.unwrap();
    assert_eq!(data["assets_path"], json!(expected));
    assert!(!expected.exists());
    let read = server
        .call(
            "article_read",
            json!({ "article": data["id"] }),
            Some(scratch.path()),
        )
        .unwrap_or_else(|_| panic!("article read failed"));
    assert_eq!(read.text, "Body");
    assert_eq!(read.data.unwrap()["assets_path"], json!(expected));
    assert!(!expected.exists());
}

/// What a new article costs: one call, and it is on disk under a title a
/// person can say back.
#[test]
fn an_article_is_written_and_found_by_its_title() {
    let scratch = Scratch::new("articles");
    let server = scratch.server();

    let made = said(server.call(
        "article_add",
        json!({
            "project": scratch.path(),
            "title": "Notes on the watch",
            "text": "# Notes\n\nIt bounces.",
        }),
        None,
    ));
    assert_eq!(made, "#1 Notes on the watch written");

    let read = said(server.call(
        "article_read",
        json!({ "project": scratch.path(), "article": "notes on the watch" }),
        None,
    ));
    assert_eq!(read, "# Notes\n\nIt bounces.");
}

/// The title is a property beside the document, so renaming one moves neither
/// the file nor what it is filed under.
#[test]
fn a_rename_leaves_the_id_alone() {
    let scratch = Scratch::new("rename");
    let server = scratch.server();
    said(server.call(
        "article_add",
        json!({ "project": scratch.path(), "title": "Draft", "text": "x" }),
        None,
    ));

    let listed = said(server.call("article_list", json!({ "project": scratch.path() }), None));
    let id = listed.split_whitespace().last().expect("an id").to_owned();

    said(server.call(
        "article_rename",
        json!({ "project": scratch.path(), "article": "Draft", "title": "Shipped" }),
        None,
    ));
    let read = said(server.call(
        "article_read",
        json!({ "project": scratch.path(), "article": &id }),
        None,
    ));
    assert_eq!(read, "x", "the same article answers to the same id");
}

/// A rewrite replaces the markdown and nothing else.
#[test]
fn a_rewrite_keeps_the_title() {
    let scratch = Scratch::new("rewrite-article");
    let server = scratch.server();
    said(server.call(
        "article_add",
        json!({ "project": scratch.path(), "title": "Roadmap", "text": "first" }),
        None,
    ));
    said(server.call(
        "article_rewrite",
        json!({ "project": scratch.path(), "article": "Roadmap", "text": "second" }),
        None,
    ));

    let read = said(server.call(
        "article_read",
        json!({ "project": scratch.path(), "article": "Roadmap" }),
        None,
    ));
    assert_eq!(read, "second");
}

/// A refusal names what is there, so a model that guessed wrong fixes it from
/// the answer rather than spending a turn asking.
#[test]
fn a_refusal_says_what_is_there() {
    let scratch = Scratch::new("articles-refusal");
    let server = scratch.server();
    said(server.call(
        "article_add",
        json!({ "project": scratch.path(), "title": "Roadmap", "text": "x" }),
        None,
    ));

    let why = refused(server.call(
        "article_read",
        json!({ "project": scratch.path(), "article": "Nothing" }),
        None,
    ));
    assert!(why.contains("Roadmap"), "{why}");

    // A directory nobody opened is not a project to work in, and the refusal
    // names what is open so a second call can be right.
    let why = refused(server.call(
        "article_list",
        json!({ "project": "/no/such/directory" }),
        None,
    ));
    assert!(
        why.contains("does not have /no/such/directory open"),
        "{why}"
    );
    assert!(why.contains(&scratch.path().display().to_string()), "{why}");

    // A binding is taken as given, so a directory that went away under one is
    // where the missing-directory answer still comes from.
    let gone = scratch.path().join("gone");
    let why = refused(server.call("article_list", json!({}), Some(&gone)));
    assert!(why.contains("no directory"), "{why}");

    let why = invalid(server.call("article_read", json!({ "project": scratch.path() }), None));
    assert!(why.contains("article"), "{why}");
}

/// An empty project says so rather than answering with a blank line.
#[test]
fn nothing_written_yet_is_said_plainly() {
    let scratch = Scratch::new("articles-empty");
    let server = scratch.server();
    let text = said(server.call("article_list", json!({ "project": scratch.path() }), None));
    assert_eq!(text, "this project has no articles");
}
