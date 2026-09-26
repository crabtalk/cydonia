//! What an install has to show for itself while it runs.

use cydonia_gui::agent::{OUTPUT_KEEP, record};

fn printed(lines: &[&str]) -> Vec<String> {
    let mut held = Vec::new();
    for line in lines {
        record(&mut held, (*line).to_owned());
    }
    held
}

/// The row reads the last line as its status, so the order the installer
/// printed in is the order this keeps.
#[test]
fn the_last_line_is_the_step_it_has_reached() {
    let held = printed(&["downloading claude.tar.gz", "downloaded 8 MB", "unpacking"]);
    assert_eq!(held.last().map(String::as_str), Some("unpacking"));
}

/// A chatty `npm` would otherwise be held in full for a row that shows one
/// line of it.
#[test]
fn a_long_install_is_held_to_its_tail() {
    let lines: Vec<String> = (0..OUTPUT_KEEP + 10).map(|n| format!("line {n}")).collect();
    let mut held = Vec::new();
    for line in lines {
        record(&mut held, line);
    }
    assert_eq!(held.len(), OUTPUT_KEEP);
    // The opening goes, not the last say: a failure's reason is at the end.
    assert_eq!(held.first().map(String::as_str), Some("line 10"));
    assert_eq!(
        held.last().map(String::as_str),
        Some(format!("line {}", OUTPUT_KEEP + 9).as_str())
    );
}

/// Nothing printed yet is a row with a spinner and no status under it, which
/// is the first moment of every install.
#[test]
fn nothing_printed_is_no_status() {
    assert!(printed(&[]).is_empty());
}
