//! 0.1.23 — `keep_pasted_images` renamed to `download_web_images`.
//!
//! Deleted with `model::migrate::v0_1_23` when that is retired.

use cydonia_gui::model::migrate::v0_1_23::rename_key;
use toml_edit::DocumentMut;

fn doc(body: &str) -> DocumentMut {
    body.parse().expect("valid toml")
}

#[test]
fn the_switch_comes_across_under_its_new_name() {
    let mut settings = doc("keep_pasted_images = false\nnotify_turns = true\n");
    assert!(rename_key(&mut settings));
    assert_eq!(settings["download_web_images"].as_bool(), Some(false));
    assert!(settings.get("keep_pasted_images").is_none());
    assert_eq!(settings["notify_turns"].as_bool(), Some(true));
}

/// Every launch runs every migration, so a second pass leaves the file alone.
#[test]
fn a_file_already_carried_is_left_as_it_is() {
    let mut settings = doc("keep_pasted_images = false\n");
    rename_key(&mut settings);
    assert!(!rename_key(&mut settings));
    assert_eq!(settings["download_web_images"].as_bool(), Some(false));
}

/// A stale old key beside the new one — a downgrade and back up — keeps what
/// the new name holds.
#[test]
fn the_new_name_wins_over_a_stale_old_one() {
    let mut settings = doc("keep_pasted_images = true\ndownload_web_images = false\n");
    assert!(rename_key(&mut settings));
    assert_eq!(settings["download_web_images"].as_bool(), Some(false));
    assert!(settings.get("keep_pasted_images").is_none());
}
