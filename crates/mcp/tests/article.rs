//! An article is written, found by the name it was given, and read back off
//! the disk the app reads.

mod common;

use common::{Scratch, invalid, refused, said, structured};
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

/// The cover is filed in the article's own folder under the `cover-` name,
/// which is what an agent could not work out for itself.
#[test]
fn a_cover_is_filed_beside_the_article_and_replaced_whole() {
    let scratch = Scratch::new("cover");
    let server = scratch.server();
    let made = server.call(
        "article_add",
        json!({ "project": scratch.path(), "title": "Notes", "text": "# Notes" }),
        None,
    );
    let folder = structured(made)["article_path"]
        .as_str()
        .expect("the article's own folder")
        .to_owned();

    let drawn = scratch.path().join("drawn.png");
    std::fs::write(&drawn, b"not really a png").unwrap();
    let text = said(server.call(
        "article_set_cover",
        json!({ "project": scratch.path(), "article": "Notes", "image": drawn.to_str() }),
        None,
    ));
    assert!(text.contains("covered"), "{text}");

    let covers = |folder: &str| {
        std::fs::read_dir(folder)
            .unwrap()
            .flatten()
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .filter(|name| name.starts_with("cover-"))
            .collect::<Vec<_>>()
    };
    let first = covers(&folder);
    assert_eq!(first.len(), 1, "one cover: {first:?}");
    assert!(first[0].ends_with(".png"), "{first:?}");

    // A second one takes the first's place rather than standing beside it.
    let other = scratch.path().join("other.jpg");
    std::fs::write(&other, b"nor this").unwrap();
    said(server.call(
        "article_set_cover",
        json!({ "project": scratch.path(), "article": "Notes", "image": other.to_str() }),
        None,
    ));
    let second = covers(&folder);
    assert_eq!(second.len(), 1, "still one cover: {second:?}");
    assert!(second[0].ends_with(".jpg"), "{second:?}");

    // And with no picture named, it comes off.
    said(server.call(
        "article_set_cover",
        json!({ "project": scratch.path(), "article": "Notes" }),
        None,
    ));
    assert!(covers(&folder).is_empty(), "the cover is gone");
}

/// A picture the host does not have is refused rather than filed empty.
#[test]
fn a_cover_that_is_not_there_is_refused() {
    let scratch = Scratch::new("cover-missing");
    let server = scratch.server();
    said(server.call(
        "article_add",
        json!({ "project": scratch.path(), "title": "Notes", "text": "# Notes" }),
        None,
    ));
    let why = refused(server.call(
        "article_set_cover",
        json!({
            "project": scratch.path(), "article": "Notes",
            "image": scratch.path().join("nothing.png").to_str()
        }),
        None,
    ));
    assert!(why.contains("no picture at"), "{why}");
}

/// Archiving is reversible and deleting is not, which is the whole reason they
/// are two tools behind two switches.
#[test]
fn an_article_is_put_away_and_then_deleted() {
    let scratch = Scratch::new("archive-article");
    let server = scratch.server();
    let made = server.call(
        "article_add",
        json!({ "project": scratch.path(), "title": "Notes", "text": "# Notes" }),
        None,
    );
    let folder = structured(made)["article_path"]
        .as_str()
        .expect("the article's own folder")
        .to_owned();

    let text = said(server.call(
        "article_archive",
        json!({ "project": scratch.path(), "article": "Notes" }),
        None,
    ));
    assert!(text.contains("put away"), "{text}");
    assert!(
        std::path::Path::new(&folder).is_dir(),
        "archived is not gone"
    );
    let listed = said(server.call("article_list", json!({ "project": scratch.path() }), None));
    assert!(listed.contains("archived"), "{listed}");

    said(server.call(
        "article_archive",
        json!({ "project": scratch.path(), "article": "Notes", "archived": false }),
        None,
    ));
    let listed = said(server.call("article_list", json!({ "project": scratch.path() }), None));
    assert!(!listed.contains("archived"), "and back again: {listed}");

    let text = said(server.call(
        "article_remove",
        json!({ "project": scratch.path(), "article": "Notes" }),
        None,
    ));
    assert!(text.contains("deleted"), "{text}");
    assert!(
        !std::path::Path::new(&folder).exists(),
        "the whole directory goes"
    );
}

/// A name that answers nothing refuses the run whole, rather than deleting the
/// ones it did find.
#[test]
fn a_run_that_names_a_stranger_deletes_none_of_it() {
    let scratch = Scratch::new("delete-run");
    let server = scratch.server();
    for title in ["First", "Second"] {
        said(server.call(
            "article_add",
            json!({ "project": scratch.path(), "title": title, "text": "body" }),
            None,
        ));
    }
    let why = refused(server.call(
        "article_remove",
        json!({ "project": scratch.path(), "article": ["First", "Nothing"] }),
        None,
    ));
    assert!(!why.is_empty(), "{why}");
    let listed = said(server.call("article_list", json!({ "project": scratch.path() }), None));
    assert!(listed.contains("First"), "still there: {listed}");
    assert!(listed.contains("Second"), "still there: {listed}");
}
