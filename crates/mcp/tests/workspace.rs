//! What the window is showing, as `workspace_focus` answers it.

mod common;

use common::{Scratch, said};
use cydonia_mcp::rail::{self, Shown};
use serde_json::json;

fn shown(scratch: &Scratch, id: &str, focused: bool) -> Shown {
    Shown {
        project: scratch.path().to_path_buf(),
        kind: "board",
        id: id.to_owned(),
        focused,
    }
}

#[test]
fn a_window_showing_nothing_says_so() {
    let scratch = Scratch::new("focus-none");
    let server = scratch.server();
    rail::set_shown(Vec::new());

    let answer = said(server.call("workspace_focus", json!({}), None));

    assert_eq!(
        answer,
        "the cydonia window is showing no article, board or table"
    );
}

#[test]
fn one_entry_is_named_without_a_focus_mark() {
    let scratch = Scratch::new("focus-one");
    let server = scratch.server();
    let board = scratch.store_create("Roadmap", "ROAD").unwrap();
    rail::set_shown(vec![shown(&scratch, &board.id, true)]);

    let answer = said(server.call("workspace_focus", json!({}), None));

    assert_eq!(
        answer,
        format!("- #1 [board] Roadmap in {}", scratch.path().display())
    );
}

#[test]
fn a_space_lists_every_pane_and_marks_the_focused_one() {
    let scratch = Scratch::new("focus-space");
    let server = scratch.server();
    let road = scratch.store_create("Roadmap", "ROAD").unwrap();
    let bugs = scratch.store_create("Bugs", "BUG").unwrap();
    rail::set_shown(vec![
        shown(&scratch, &road.id, false),
        shown(&scratch, &bugs.id, true),
    ]);

    let answer = said(server.call("workspace_focus", json!({}), None));

    let at = scratch.path().display();
    assert_eq!(
        answer,
        format!("- #1 [board] Roadmap in {at}\n- #2 [board] Bugs in {at} (focused)")
    );
}
