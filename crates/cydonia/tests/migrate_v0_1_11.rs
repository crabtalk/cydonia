//! 0.1.11 — the two keys a layout was named by, renamed to what a space is
//! named by.
//!
//! Deleted with `model::migrate::v0_1_11` when that is retired.

use cydonia::model::migrate::v0_1_11::rename_keys;
use toml_edit::DocumentMut;

fn doc(body: &str) -> DocumentMut {
    body.parse().expect("valid toml")
}

/// A state file as 0.1.10 wrote it.
fn legacy() -> DocumentMut {
    doc(r#"
projects = ["/Users/someone/work"]
active = 0
layout = "1789769788615"
layouts = ["1789769788615", "1789769788999"]
"#)
}

#[test]
fn the_open_one_and_the_order_both_come_across() {
    let mut state = legacy();
    assert!(rename_keys(&mut state));

    assert_eq!(state["space"].as_str(), Some("1789769788615"));
    let order = state["spaces"].as_array().expect("an array");
    assert_eq!(order.len(), 2);
    assert!(state.get("layout").is_none(), "and the old names go");
    assert!(state.get("layouts").is_none());
}

/// Every launch runs every migration, so the second pass over a file the first
/// one already moved has to leave it alone.
#[test]
fn a_file_already_carried_is_left_as_it_is() {
    let mut state = legacy();
    rename_keys(&mut state);
    assert!(!rename_keys(&mut state), "nothing left to rename");
    assert_eq!(state["space"].as_str(), Some("1789769788615"));
}

/// A file written since the move that still has an old key — a downgrade and
/// back up — keeps what the new name holds.
#[test]
fn the_new_name_wins_over_a_stale_old_one() {
    let mut state = doc(
        r#"
layout = "old"
space = "new"
"#,
    );
    assert!(rename_keys(&mut state));
    assert_eq!(state["space"].as_str(), Some("new"));
    assert!(state.get("layout").is_none());
}

/// `right-panels.json` as 0.1.10 wrote it: a panel per session, inside a map
/// per project.
const LEGACY_PANELS: &str = r#"{
  "width": 380.0,
  "projects": {
    "/Users/someone/work": {
      "1789769788615": { "open": true, "tabs": [], "active": null,
                         "files_open": false, "files_width": 220.0 }
    }
  }
}"#;

/// The same file once 0.1.11 has written it: a panel per working directory.
const CURRENT_PANELS: &str = r#"{
  "width": 380.0,
  "projects": {
    "/Users/someone/work": { "open": true, "tabs": [], "active": null,
                             "files_open": false, "files_width": 220.0 }
  }
}"#;

#[test]
fn the_panels_a_session_held_are_dropped_and_the_width_is_kept() {
    assert_eq!(
        cydonia::model::migrate::v0_1_11::legacy_width(LEGACY_PANELS),
        Some(Some(380.0))
    );
}

#[test]
fn a_file_already_carried_is_left_alone() {
    assert_eq!(
        cydonia::model::migrate::v0_1_11::legacy_width(CURRENT_PANELS),
        None,
        "a second pass must not throw away a panel written since"
    );
    assert_eq!(
        cydonia::model::migrate::v0_1_11::legacy_width(r#"{"projects":{}}"#),
        None,
        "a file with no panels in it is not the old shape"
    );
}
