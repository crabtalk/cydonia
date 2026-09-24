//! The chords: what the file resolves to, what the system will take, and what
//! the two of them refuse.

use cydonia::{
    model::settings::{Settings, Shortcuts},
    view::{
        hotkey,
        keymap::{self, Command},
    },
};

/// A `[shortcuts]` table as the file would hold it.
fn shortcuts(body: &str) -> Shortcuts {
    toml::from_str(body).expect("valid toml")
}

#[test]
fn a_command_nobody_moved_keeps_its_default() {
    let held = shortcuts("");
    assert_eq!(
        keymap::chord(Command::ToggleSidebar, &held),
        Some("secondary-b")
    );
    // And one that ships with no chord at all still has none.
    assert_eq!(keymap::chord(Command::NewBoard, &held), None);
}

/// Stepping is the whole of how a pane is reached by key — the four commands
/// that jumped straight to one are gone. The chords for it are ⌃⇥ and ⇧⌃⇥,
/// bound in `root::bindings` rather than shipped as defaults here: a default
/// would also go in the menu as a key equivalent, which AppKit claims before
/// the window is offered the chord.
#[test]
fn stepping_between_entries_ships_no_chord_of_its_own() {
    let bare = Shortcuts::default();
    assert_eq!(keymap::chord(Command::NextEntry, &bare), None);
    assert_eq!(keymap::chord(Command::PrevEntry, &bare), None);
    assert!(!Command::ALL.iter().any(|command| command.title() == "Chat"));
}

#[test]
fn the_file_wins_over_the_default() {
    let held = shortcuts(r#"toggle_sidebar = "cmd-shift-b""#);
    assert_eq!(
        keymap::chord(Command::ToggleSidebar, &held),
        Some("cmd-shift-b")
    );
}

/// The only way to say "nothing", since absence is already the default.
#[test]
fn an_empty_chord_is_a_command_that_answers_to_nothing() {
    let held = shortcuts(r#"toggle_sidebar = """#);
    assert_eq!(keymap::chord(Command::ToggleSidebar, &held), None);
}

/// The line stays in the file where it can be seen and fixed, and the command
/// goes on working in the meantime.
#[test]
fn a_chord_that_is_not_one_falls_back_rather_than_unbinding() {
    let held = shortcuts(r#"toggle_sidebar = "cmd-nonsense-key""#);
    assert_eq!(
        keymap::chord(Command::ToggleSidebar, &held),
        Some("secondary-b")
    );
}

/// Two commands on one chord is not an error the keymap raises — the later
/// binding wins and the other goes quiet — so the section has to ask.
#[test]
fn a_chord_already_spoken_for_names_who_has_it() {
    let held = shortcuts(r#"new_board = "secondary-b""#);
    assert_eq!(
        keymap::claimed("secondary-b", Command::NewBoard, &held),
        vec![Command::ToggleSidebar]
    );
    // Symmetric, and neither of them counts itself: each is told who else is
    // on the chord, which is what a row has to say.
    assert_eq!(
        keymap::claimed("secondary-b", Command::ToggleSidebar, &held),
        vec![Command::NewBoard]
    );
}

/// A recorder writes the platform's own modifier, and the defaults are written
/// with `secondary`: the two spellings are one chord.
#[test]
fn a_recorded_chord_meets_the_default_it_spells_differently() {
    let recorded = if cfg!(target_os = "macos") {
        "cmd-b"
    } else {
        "ctrl-b"
    };
    assert_eq!(
        keymap::claimed(recorded, Command::NewBoard, &shortcuts("")),
        vec![Command::ToggleSidebar]
    );
}

#[test]
fn every_command_writes_itself_under_a_key_of_its_own() {
    let mut keys: Vec<&str> = Command::ALL.into_iter().map(Command::key).collect();
    keys.sort_unstable();
    let count = keys.len();
    keys.dedup();
    assert_eq!(keys.len(), count);
    // And none of them is the one key in the table that is not a command.
    assert!(!keys.contains(&Shortcuts::ACTIVATE));
}

/// What ships must not collide with itself: a default landing on another
/// command's default would leave one of them dead out of the box.
#[test]
fn no_two_defaults_want_the_same_chord() {
    let bare = Shortcuts::default();
    for command in Command::ALL {
        assert!(
            keymap::claimed(
                keymap::chord(command, &bare).unwrap_or_default(),
                command,
                &bare
            )
            .is_empty(),
            "{} collides",
            command.title()
        );
    }
}

#[test]
fn chords_are_written_the_way_the_platform_writes_them() {
    assert_eq!(keymap::glyphs("cmd-b").as_deref(), Some("⌘B"));
    assert_eq!(keymap::glyphs("ctrl-alt-cmd-1").as_deref(), Some("⌃⌥⌘1"));
    assert_eq!(keymap::glyphs("not-a-chord"), None);
}

#[test]
fn the_system_takes_a_modified_single_stroke_and_nothing_else() {
    assert!(hotkey::holdable("ctrl-space"));
    assert!(hotkey::holdable("alt-cmd-,"));
    assert!(hotkey::holdable("shift-cmd-f7"));
    // A bare key would be that key, everywhere, for every app on the desktop.
    assert!(!hotkey::holdable("space"));
    // Carbon registers one combination and has nowhere to put the waiting.
    assert!(!hotkey::holdable("cmd-k cmd-b"));
    assert!(!hotkey::holdable(""));
}

/// A fresh install claims no key across the desktop, and a table that says
/// nothing is not written as a table full of defaults.
#[test]
fn a_fresh_file_holds_no_shortcuts_at_all() {
    let fresh = Settings::default();
    assert_eq!(fresh.shortcuts.activate(), None);
    let body = toml::to_string_pretty(&fresh).expect("serialises");
    assert!(body.contains("[shortcuts]"), "{body}");
    for command in Command::ALL {
        assert!(!body.contains(command.key()), "{body}");
    }
    // And it reads back as what it was.
    let read: Settings = toml::from_str(&body).expect("round trips");
    assert_eq!(read.shortcuts.activate(), None);
    assert_eq!(
        keymap::chord(Command::ToggleSidebar, &read.shortcuts),
        Some("secondary-b")
    );
}
