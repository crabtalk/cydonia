mod common;

use common::Scratch;
use cydonia_artifact::entry::Registry;

#[test]
fn entries_db_becomes_state_db_with_its_numbers() {
    let scratch = Scratch::new("stats-migrate");
    let mut registry = Registry::open(scratch.path()).unwrap();
    let number = registry.number("article", "kept").unwrap();
    drop(registry);
    let dir = scratch.path().join(".cydonia");
    std::fs::rename(dir.join("state.db"), dir.join("entries.db")).unwrap();

    let mut registry = Registry::open(scratch.path()).unwrap();
    assert_eq!(registry.number("article", "kept").unwrap(), number);
    assert!(dir.join("state.db").is_file());
    assert!(!dir.join("entries.db").exists());
}
