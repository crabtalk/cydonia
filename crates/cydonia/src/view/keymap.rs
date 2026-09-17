//! The keymap, put together in one place.
//!
//! Every chord the app answers to arrives here: bezel's own defaults for the
//! fields and the editor, this crate's context-scoped keys, and last the
//! commands a person may move — see [`Command`].
//!
//! Last is what makes them movable. gpui breaks a tie between two bindings on
//! the same chord by insertion order, so a command bound after bezel's
//! defaults wins the chord from them; and [`bind_all`] clears the keymap
//! before it runs, so a rebind is this list assembled again rather than a
//! patch laid over what was already there. `gpui::NoAction` would have been
//! the smaller move and is the wrong one: bound app-wide it takes the chord
//! from the editor too, which is the opposite of what freeing ⌘B is for.
//!
//! A rebind goes through [`rebind`] and not through [`bind_all`] alone,
//! because the menu bar holds its own copy of every equivalent — see
//! [`menubar::refresh`].

use crate::{
    model::settings::Shortcuts,
    view::{
        article::{self, TogglePlainText},
        board,
        component::{composer, ribbon, terminal},
        create, info, menubar,
        root::{
            self, CloseProject, NewArticle, NewBoard, NewSession, NewSessionNext, NewTable,
            NextEntry, OpenFiles, OpenProject, OpenReview, OpenSettings, PrevEntry, ToggleChanges,
            ToggleSidebar, ToggleTerminal,
        },
        table,
    },
};
use bezel::{
    gpui::{App, DummyKeyboardMapper, KeyBinding, KeybindingKeystroke, Keystroke, SharedString},
    ui::{focus, input, keys},
};

/// One command whose chord is the reader's to choose.
///
/// Named rather than reached as a field, so the settings section can list them
/// and one writer can put any of them in the file — the shape
/// [`crate::model::settings::Feature`] already uses.
///
/// What is *not* here is as decided as what is. The context-scoped keys —
/// `enter` in a card, `escape` in a dialog — cannot go on the menu bar at all
/// (see [`menubar`]) and are the surface's grammar rather than a preference.
/// ⌘Q, ⌘W, ⌘H and ⌘M are where macOS puts them for every app, and an app that
/// let you move them would be the only one you had to remember.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    OpenSettings,
    NewSession,
    NewSessionNext,
    NewBoard,
    NewArticle,
    NewTable,
    OpenProject,
    CloseProject,
    ToggleSidebar,
    ToggleTerminal,
    ToggleChanges,
    OpenFiles,
    OpenReview,
    NextEntry,
    PrevEntry,
    PlainText,
}

/// Which menu a command is reached by, so the section is read in the order the
/// bar is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Menu {
    App,
    File,
    View,
}

impl Menu {
    pub const ALL: [Self; 3] = [Self::App, Self::File, Self::View];

    pub fn title(self) -> &'static str {
        match self {
            // The menu's own name, which a bundle takes from `CFBundleName`.
            Self::App => "cydonia",
            Self::File => "File",
            Self::View => "View",
        }
    }
}

impl Command {
    pub const ALL: [Self; 16] = [
        Self::OpenSettings,
        Self::NewSession,
        Self::NewSessionNext,
        Self::NewBoard,
        Self::NewArticle,
        Self::NewTable,
        Self::OpenProject,
        Self::CloseProject,
        Self::ToggleSidebar,
        Self::ToggleTerminal,
        Self::ToggleChanges,
        Self::OpenFiles,
        Self::OpenReview,
        Self::NextEntry,
        Self::PrevEntry,
        Self::PlainText,
    ];

    /// The key it is written under, inside `[shortcuts]`.
    pub fn key(self) -> &'static str {
        match self {
            Self::OpenSettings => "open_settings",
            Self::NewSession => "new_session",
            Self::NewSessionNext => "new_session_next",
            Self::NewBoard => "new_board",
            Self::NewArticle => "new_article",
            Self::NewTable => "new_table",
            Self::OpenProject => "open_project",
            Self::CloseProject => "close_project",
            Self::ToggleSidebar => "toggle_sidebar",
            Self::ToggleTerminal => "toggle_terminal",
            Self::ToggleChanges => "toggle_changes",
            Self::OpenFiles => "open_files",
            Self::OpenReview => "open_review",
            Self::NextEntry => "next_entry",
            Self::PrevEntry => "prev_entry",
            Self::PlainText => "plain_text",
        }
    }

    /// What the menu bar calls it, so the two lists name one thing.
    pub fn title(self) -> &'static str {
        match self {
            Self::OpenSettings => "Settings…",
            Self::NewSession => "New Session",
            Self::NewSessionNext => "New Session on Next Agent",
            Self::NewBoard => "New Board",
            Self::NewArticle => "New Article",
            Self::NewTable => "New Table",
            Self::OpenProject => "Open Project…",
            Self::CloseProject => "Close Project",
            Self::ToggleSidebar => "Toggle Sidebar",
            Self::ToggleTerminal => "Toggle Terminal",
            Self::ToggleChanges => "Toggle Right Panel",
            Self::OpenFiles => "Open Files",
            Self::OpenReview => "Open Review",
            Self::NextEntry => "Next Entry",
            Self::PrevEntry => "Previous Entry",
            Self::PlainText => "Plain Text",
        }
    }

    pub fn menu(self) -> Menu {
        match self {
            Self::OpenSettings => Menu::App,
            Self::NewSession
            | Self::NewSessionNext
            | Self::NewBoard
            | Self::NewArticle
            | Self::NewTable
            | Self::OpenProject
            | Self::CloseProject => Menu::File,
            Self::ToggleTerminal
            | Self::ToggleChanges
            | Self::OpenFiles
            | Self::OpenReview
            | Self::ToggleSidebar
            | Self::NextEntry
            | Self::PrevEntry
            | Self::PlainText => Menu::View,
        }
    }

    /// The chord it ships with, where it ships with one. The four that do not
    /// are reachable from the menu and were never worth a default between
    /// them: ⌘N is the session, and three more `New` chords would spend the
    /// letters a person may want for their own.
    pub fn default(self) -> Option<&'static str> {
        Some(match self {
            // What macOS binds Preferences to in every other app.
            Self::OpenSettings => "cmd-,",
            Self::NewSession => "cmd-n",
            // ⌘N's other agent: the chord a second agent needs, since ⌘N
            // stays with whoever the project last talked to.
            Self::NewSessionNext => "alt-cmd-n",
            Self::NewBoard | Self::NewArticle | Self::NewTable | Self::CloseProject => return None,
            Self::OpenProject => "cmd-o",
            // What every app with a sidebar binds it to.
            Self::ToggleSidebar => "cmd-b",
            Self::ToggleTerminal => "cmd-j",
            Self::ToggleChanges => "cmd-l",
            Self::OpenFiles => "cmd-shift-f",
            Self::OpenReview => "cmd-shift-g",
            // The pair the View menu draws. `ctrl-tab` reaches these too and
            // is not movable: gpui has no macOS equivalent for `tab`, so an
            // item naming it would print ⌃T — see [`root::bindings`].
            Self::NextEntry => "alt-cmd-right",
            Self::PrevEntry => "alt-cmd-left",
            Self::PlainText => "cmd-e",
        })
    }

    /// The binding this command answers to, or nothing where it answers to no
    /// chord at all.
    fn binding(self, chord: Option<&str>) -> Option<KeyBinding> {
        let chord = chord?;
        Some(match self {
            Self::OpenSettings => KeyBinding::new(chord, OpenSettings, None),
            Self::NewSession => KeyBinding::new(chord, NewSession, None),
            Self::NewSessionNext => KeyBinding::new(chord, NewSessionNext, None),
            Self::NewBoard => KeyBinding::new(chord, NewBoard, None),
            Self::NewArticle => KeyBinding::new(chord, NewArticle, None),
            Self::NewTable => KeyBinding::new(chord, NewTable, None),
            Self::OpenProject => KeyBinding::new(chord, OpenProject, None),
            Self::CloseProject => KeyBinding::new(chord, CloseProject, None),
            Self::ToggleSidebar => KeyBinding::new(chord, ToggleSidebar, None),
            Self::ToggleTerminal => KeyBinding::new(chord, ToggleTerminal, None),
            Self::ToggleChanges => KeyBinding::new(chord, ToggleChanges, None),
            Self::OpenFiles => KeyBinding::new(chord, OpenFiles, None),
            Self::OpenReview => KeyBinding::new(chord, OpenReview, None),
            Self::NextEntry => KeyBinding::new(chord, NextEntry, None),
            Self::PrevEntry => KeyBinding::new(chord, PrevEntry, None),
            Self::PlainText => KeyBinding::new(chord, TogglePlainText, None),
        })
    }
}

/// What `command` answers to right now: what the file says, or its default
/// where the file says nothing.
///
/// An empty string is how the file says *nothing at all* — the way to take a
/// default away, which absence cannot express because absence is the default.
/// A chord that does not parse is treated the same as one nobody wrote: the
/// line stays in the file where it can be seen and fixed, and the command
/// keeps working in the meantime.
pub fn chord(command: Command, shortcuts: &Shortcuts) -> Option<&str> {
    match shortcuts.get(command.key()) {
        Some("") => None,
        Some(chord) if parses(chord) => Some(chord),
        Some(_) | None => command.default(),
    }
}

/// Whether `chord` is one gpui can bind. Asked before it reaches
/// [`KeyBinding::new`], which panics on what it cannot parse — and this file
/// is written by hand.
pub fn parses(chord: &str) -> bool {
    let mut strokes = chord.split_whitespace().peekable();
    strokes.peek().is_some() && strokes.all(|stroke| Keystroke::parse(stroke).is_ok())
}

/// How a chord is written on screen: `cmd-shift-b` as ⇧⌘B.
pub fn glyphs(chord: &str) -> Option<SharedString> {
    let strokes = chord
        .split_whitespace()
        .map(|stroke| {
            Keystroke::parse(stroke).ok().map(|stroke| {
                KeybindingKeystroke::new_with_mapper(stroke, false, &DummyKeyboardMapper)
            })
        })
        .collect::<Option<Vec<_>>>()?;
    (!strokes.is_empty()).then(|| keys::format(&strokes))
}

/// How a command's chord is written on screen right now, for a control that
/// names it — a tooltip, a menu row.
///
/// Read off the same table [`bind_all`] is built from, so a label cannot drift
/// from the chord that fires: a control with its chord typed beside it goes on
/// saying ⌘B long after ⌘B was moved.
pub fn label(command: Command, shortcuts: &Shortcuts) -> Option<SharedString> {
    glyphs(chord(command, shortcuts)?)
}

/// The commands that already answer to `chord`, minus `mine`.
///
/// One chord on two commands is not an error the keymap reports — the later
/// binding simply wins and the earlier one goes quiet — so the section has to
/// ask before it writes.
pub fn claimed(wanted: &str, mine: Command, shortcuts: &Shortcuts) -> Vec<Command> {
    Command::ALL
        .into_iter()
        .filter(|&command| command != mine && chord(command, shortcuts) == Some(wanted))
        .collect()
}

/// Optional word shortcuts, scoped to editable text.
fn emacs_bindings() -> Vec<KeyBinding> {
    vec![
        KeyBinding::new("alt-b", input::WordLeft, Some(input::KEY_CONTEXT)),
        KeyBinding::new("alt-f", input::WordRight, Some(input::KEY_CONTEXT)),
        KeyBinding::new(
            "alt-shift-b",
            input::SelectWordLeft,
            Some(input::KEY_CONTEXT),
        ),
        KeyBinding::new(
            "alt-shift-f",
            input::SelectWordRight,
            Some(input::KEY_CONTEXT),
        ),
        KeyBinding::new("alt-d", input::DeleteWordRight, Some(input::KEY_CONTEXT)),
        KeyBinding::new("alt-b", editor::keys::WordLeft, Some(editor::CONTEXT)),
        KeyBinding::new("alt-f", editor::keys::WordRight, Some(editor::CONTEXT)),
        KeyBinding::new(
            "alt-shift-b",
            editor::keys::SelectWordLeft,
            Some(editor::CONTEXT),
        ),
        KeyBinding::new(
            "alt-shift-f",
            editor::keys::SelectWordRight,
            Some(editor::CONTEXT),
        ),
        KeyBinding::new(
            "alt-d",
            editor::keys::DeleteWordRight,
            Some(editor::CONTEXT),
        ),
    ]
}

/// Put the whole keymap in, from bezel's defaults up to the commands.
///
/// Cleared first, so calling it twice leaves the same keymap as calling it
/// once — which is what makes a rebind possible without a restart.
pub fn bind_all(shortcuts: &Shortcuts, cx: &mut App) {
    cx.clear_key_bindings();
    // bezel's, which this crate has rather than takes: `init` on each of them
    // is this same list bound, and going through the data is what lets a
    // command below win a chord one of them holds.
    cx.bind_keys(input::bindings());
    cx.bind_keys(focus::bindings());
    cx.bind_keys(editor::keys::bindings());
    if shortcuts.emacs {
        cx.bind_keys(emacs_bindings());
    }
    // The app's own, every one of them scoped to a surface.
    cx.bind_keys(article::bindings());
    cx.bind_keys(board::bindings());
    cx.bind_keys(composer::bindings());
    cx.bind_keys(create::bindings());
    cx.bind_keys(info::bindings());
    cx.bind_keys(ribbon::bindings());
    cx.bind_keys(table::bindings());
    cx.bind_keys(terminal::bindings());
    cx.bind_keys([
        KeyBinding::new(
            "cmd-f",
            super::component::files::ToggleFilter,
            Some("SessionPanel"),
        ),
        KeyBinding::new(
            "cmd-p",
            super::component::panel::OpenFile,
            Some("SessionPanel"),
        ),
        KeyBinding::new(
            "cmd-w",
            super::component::panel::CloseTab,
            Some("SessionPanel || BottomTerminalPanel"),
        ),
        KeyBinding::new(
            "cmd-t",
            super::component::panel::NewTerminal,
            Some("SessionPanel || BottomTerminalPanel"),
        ),
        // The chord that steps between entries everywhere else steps between
        // tabs here — see [`root::bindings`], whose binding this shadows while
        // a panel holds the focus. `cmd-tab` is the system's and never reaches
        // an app; the terminal makes no bytes of `ctrl-tab`, so a shell under
        // the pointer does not eat it either.
        KeyBinding::new(
            "ctrl-tab",
            super::component::panel::NextTab,
            Some("SessionPanel || BottomTerminalPanel"),
        ),
        KeyBinding::new(
            "ctrl-shift-tab",
            super::component::panel::PrevTab,
            Some("SessionPanel || BottomTerminalPanel"),
        ),
        KeyBinding::new("cmd-s", super::component::file::Save, Some("FileEditor")),
        KeyBinding::new("cmd-c", input::Copy, Some("FileEditor")),
        KeyBinding::new("cmd-a", input::SelectAll, Some("FileEditor")),
        KeyBinding::new(
            "cmd-=",
            super::component::file::IncreaseTextSize,
            Some("FileEditor"),
        ),
        KeyBinding::new(
            "cmd-+",
            super::component::file::IncreaseTextSize,
            Some("FileEditor"),
        ),
        KeyBinding::new(
            "cmd-shift-=",
            super::component::file::IncreaseTextSize,
            Some("FileEditor"),
        ),
        KeyBinding::new(
            "cmd--",
            super::component::file::DecreaseTextSize,
            Some("FileEditor"),
        ),
        KeyBinding::new(
            "cmd-0",
            super::component::file::ResetTextSize,
            Some("FileEditor"),
        ),
    ]);
    cx.bind_keys(root::bindings());
    cx.bind_keys(menubar::bindings());
    // Last, and the only ones the reader can move.
    cx.bind_keys(
        Command::ALL
            .into_iter()
            .filter_map(|command| command.binding(chord(command, shortcuts)))
            .collect::<Vec<_>>(),
    );
}

/// Assemble the keymap again and hand the menu bar what it now says.
///
/// The second half is not decoration. AppKit holds the key equivalent it was
/// given and claims the chord before gpui is offered it, so a rebind that left
/// the menus alone would leave the old chord firing and the new one dead.
pub fn rebind(shortcuts: &Shortcuts, cx: &mut App) {
    bind_all(shortcuts, cx);
    menubar::refresh(cx);
}
