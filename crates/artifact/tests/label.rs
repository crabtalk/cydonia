//! A project's `labels.toml`: what it says of each label, and edits that keep
//! what they do not touch.

use cydonia_artifact::{
    label::{self, Label},
    project::{Project as _, memory},
};

const FILE: &str = r##"# kept by hand
[bug]
color = "red" # loud
description = "Something is broken"
owner = "ops"

[q3]
color = "#e5534b"

[draft]
"##;

fn label(color: Option<&str>, description: Option<&str>) -> Label {
    Label {
        color: color.map(str::to_owned),
        description: description.map(str::to_owned),
    }
}

#[test]
fn every_table_is_read_even_an_empty_one() {
    let labels = label::read(FILE);
    assert_eq!(
        labels["bug"],
        label(Some("red"), Some("Something is broken"))
    );
    assert_eq!(labels["q3"], label(Some("#e5534b"), None));
    assert_eq!(labels["draft"], Label::default());
}

#[test]
fn text_that_does_not_parse_holds_none_and_is_left_alone() {
    let broken = "[bug\ncolor = ";
    assert!(label::read(broken).is_empty());
    assert_eq!(label::save(broken, "bug", &Label::default()), broken);
}

#[test]
fn saving_keeps_comments_and_other_keys() {
    let text = label::save(FILE, "bug", &label(Some("green"), None));
    assert_eq!(label::read(&text)["bug"], label(Some("green"), None));
    assert!(text.contains("# kept by hand"));
    assert!(text.contains("owner = \"ops\""));
}

#[test]
fn saving_a_new_name_registers_it_with_nothing_in_it() {
    let text = label::save(FILE, "later", &Label::default());
    assert_eq!(label::read(&text)["later"], Label::default());
}

#[test]
fn a_rename_moves_the_table_and_keeps_what_the_target_has() {
    let text = label::rename(FILE, "q3", "release");
    assert_eq!(label::read(&text)["release"], label(Some("#e5534b"), None));
    assert!(!text.contains("[q3]"));
    let text = label::rename(FILE, "bug", "q3");
    assert_eq!(
        label::read(&text)["q3"],
        label(Some("#e5534b"), Some("Something is broken"))
    );
}

#[test]
fn a_removal_drops_the_table() {
    let text = label::remove(FILE, "bug");
    assert!(!label::read(&text).contains_key("bug"));
    assert!(label::read(&text).contains_key("q3"));
}

#[test]
fn the_memory_backend_seeds_and_keeps_the_file() {
    let store = memory::Project::seed([("labels.toml", FILE.as_bytes())]);
    assert_eq!(store.labels(), FILE);
    store.save_labels("[x]\ncolor = \"red\"\n").unwrap();
    assert_eq!(label::read(&store.labels())["x"], label(Some("red"), None));
}
