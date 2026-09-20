//! What an agent can do to the rail: put a directory on it, making the
//! directory where there is none, and take one off again.

mod common;

use common::{Rail, Scratch, refused, said};
use cydonia_mcp::rail::Change;
use serde_json::json;

/// The whole of what `project_open` does itself: a directory that was not there
/// is there afterwards, and the app is asked for the rest.
#[test]
fn a_directory_that_is_not_there_is_made_and_opened() {
    let scratch = Scratch::new("open-new");
    let rail = Rail::holding(&[]);
    let server = scratch.server();
    let fresh = scratch.path().join("not-yet");

    let made = said(server.call("project_open", json!({ "path": fresh }), None));

    assert!(fresh.is_dir());
    // Answered under the path the app will hold it by: `/var` is a symlink on
    // macOS, and two spellings of one directory would be two projects.
    let settled = fresh.canonicalize().unwrap();
    assert_eq!(made, format!("made and opened {}", settled.display()));
    assert!(rail.was_asked(Change::Open(settled)));
}

/// A directory that is already there is opened and not touched — and one
/// already on the rail is brought forward, which is what the app does with it.
#[test]
fn a_directory_that_is_there_is_opened_as_it_stands() {
    let scratch = Scratch::new("open-held");
    let held = scratch.path().canonicalize().unwrap();
    let rail = Rail::holding(&[&held]);
    let server = scratch.server();

    let said = said(server.call("project_open", json!({ "path": held }), None));

    assert_eq!(said, format!("brought forward {}", held.display()));
    assert!(rail.was_asked(Change::Open(held)));
}

/// A path that is a file is a mistake to report. Nothing is made over it, and
/// the rail is not asked about it.
#[test]
fn a_file_is_not_a_project() {
    let scratch = Scratch::new("open-file");
    let rail = Rail::holding(&[]);
    let server = scratch.server();
    let file = scratch.path().join("notes.md");
    std::fs::write(&file, "x").unwrap();

    let why = refused(server.call("project_open", json!({ "path": file }), None));

    assert!(why.contains("not a directory"), "{why}");
    assert!(file.is_file());
    assert!(!rail.asked().iter().any(|change| match change {
        Change::Open(path) | Change::Close(path) => path == &file,
    }));
}

/// A relative path means something only to a caller that is in a project, and
/// the door's own working directory is not it.
#[test]
fn a_relative_path_needs_a_project_to_read_it_against() {
    let scratch = Scratch::new("open-relative");
    let _rail = Rail::holding(&[]);
    let server = scratch.server();

    let why = refused(server.call("project_open", json!({ "path": "sub" }), None));
    assert!(why.contains("whole path"), "{why}");

    // The same call from a session that is in one lands beside it.
    let made = said(server.call(
        "project_open",
        json!({ "path": "sub" }),
        Some(scratch.path()),
    ));
    assert!(scratch.path().join("sub").is_dir());
    assert!(made.ends_with("sub"), "{made}");
}

/// Closing takes it off the rail and leaves every file where it is.
#[test]
fn closing_a_project_leaves_the_directory_alone() {
    let scratch = Scratch::new("close");
    let held = scratch.path().canonicalize().unwrap();
    let rail = Rail::holding(&[&held]);
    let server = scratch.server();

    let said = said(server.call("project_close", json!({ "path": held }), None));

    assert_eq!(said, format!("closed {}", held.display()));
    assert!(rail.was_asked(Change::Close(held)));
    assert!(scratch.path().is_dir());
}

/// A project that is not open cannot be closed, and the refusal says what is —
/// a model that named the wrong directory can see the right one from here.
#[test]
fn closing_what_is_not_open_says_what_is() {
    let scratch = Scratch::new("close-absent");
    let held = scratch.path().canonicalize().unwrap();
    let _rail = Rail::holding(&[&held]);
    let server = scratch.server();
    let stranger = scratch.path().join("elsewhere");
    std::fs::create_dir_all(&stranger).unwrap();

    let why = refused(server.call("project_close", json!({ "path": stranger }), None));

    assert!(why.contains("is not open"), "{why}");
    assert!(why.contains(&held.display().to_string()), "{why}");
}

/// Several projects close in one call, and one of them that is not open
/// refuses the whole call — nothing is asked for until every path has checked
/// out, so a list with a typo in it leaves the rail as it was.
#[test]
fn several_projects_close_in_one_call() {
    let one = Scratch::new("close-batch-one");
    let two = Scratch::new("close-batch-two");
    let held = one.path().canonicalize().unwrap();
    let also = two.path().canonicalize().unwrap();
    let rail = Rail::holding(&[&held, &also]);
    let server = one.server();
    let stranger = one.path().join("elsewhere");
    std::fs::create_dir_all(&stranger).unwrap();

    let why = refused(server.call("project_close", json!({ "path": [&held, &stranger] }), None));
    assert!(why.contains("is not open"), "{why}");
    assert!(!rail.was_asked(Change::Close(held.clone())));

    let text = said(server.call("project_close", json!({ "path": [&held, &also] }), None));

    assert_eq!(
        text,
        format!("closed {}, {}", held.display(), also.display())
    );
    assert!(rail.was_asked(Change::Close(held)));
    assert!(rail.was_asked(Change::Close(also)));
}
