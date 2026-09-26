//! A save based on a read the backend has since moved past is refused, on
//! every backend, and a fresh read saves.

mod common;

use common::Scratch;
use cydonia_artifact::project::{Project, Stale, memory};

fn two_writers(store: &impl Project) {
    let made = store.create_board("Plans", "PLAN").unwrap();
    let mut first = store.board(&made.id).unwrap();
    let mut second = store.board(&made.id).unwrap();

    first.name = "First".into();
    store.save_board(&mut first).unwrap();

    second.name = "Second".into();
    let refused = store.save_board(&mut second).unwrap_err();
    assert!(refused.is::<Stale>(), "{refused}");
    assert_eq!(store.board(&made.id).unwrap().name, "First");

    let mut fresh = store.board(&made.id).unwrap();
    fresh.name = "Second".into();
    store.save_board(&mut fresh).unwrap();
    assert_eq!(store.board(&made.id).unwrap().name, "Second");

    // The copy that saved carries the version it wrote, so it saves again.
    fresh.name = "Third".into();
    store.save_board(&mut fresh).unwrap();
}

fn gone(store: &impl Project) {
    let made = store.create_board("Plans", "PLAN").unwrap();
    let mut held = store.board(&made.id).unwrap();
    store.remove_board(&made.id).unwrap();
    assert!(store.save_board(&mut held).unwrap_err().is::<Stale>());
    assert!(store.board(&made.id).is_none());
}

#[test]
fn files_refuse_a_stale_save() {
    let scratch = Scratch::new("stale-fs");
    two_writers(&scratch.store());
}

#[test]
fn files_refuse_to_resurrect_a_removed_board() {
    let scratch = Scratch::new("stale-fs-gone");
    gone(&scratch.store());
}

#[test]
fn memory_refuses_a_stale_save() {
    two_writers(&memory::Project::new());
}

#[test]
fn memory_refuses_to_resurrect_a_removed_board() {
    gone(&memory::Project::new());
}

/// Another writer that does not go through a backend at all — an agent with
/// the file open — is caught the same way.
#[test]
fn a_hand_edit_makes_a_held_copy_stale() {
    let scratch = Scratch::new("stale-hand");
    let store = scratch.store();
    let made = store.create_board("Plans", "PLAN").unwrap();
    let mut held = store.board(&made.id).unwrap();
    let file = scratch
        .path()
        .join(".cydonia/boards")
        .join(format!("{}.toml", made.id));
    let text = std::fs::read_to_string(&file).unwrap();
    std::fs::write(&file, text.replace("Plans", "Edited")).unwrap();
    held.name = "Mine".into();
    assert!(store.save_board(&mut held).unwrap_err().is::<Stale>());
    assert_eq!(store.board(&made.id).unwrap().name, "Edited");
}
