//! The macOS menu bar: the tree, the commands only it names, and the wiring
//! that decides which of them are live.
//!
//! An item carries an action and a name, never a shortcut. `set_menus` reads
//! the equivalent off the keymap, so the `bind_keys` in each view's `init`
//! stays the one place a chord is written — at the price of two rules:
//!
//! * every `init` runs before this module does, or an item is built for an
//!   action whose binding is not registered yet and shows no shortcut at all;
//! * an action bound inside a key context does not belong here. AppKit claims
//!   a key equivalent before gpui sees the keystroke and dispatches it straight
//!   at the focused path with the context unread, so an item for the composer's
//!   `enter` would make `enter` mean "send" in every field in the app.
//!
//! Which items are live is not written here either. macOS validates each one
//! against [`bezel::gpui::App::is_action_available`] on every open and before
//! every key equivalent, so an item greys itself exactly when nothing in the
//! focused path handles its action — which is what [`Cydonia::commands`] is
//! for, and why a greyed item's shortcut still reaches the keymap underneath.

use crate::view::root::{
    CloseProject, Cydonia, NewArticle, NewBoard, NewSession, NewTable, NextEntry, OpenProject,
    OpenSettings, Pane, PrevEntry, ShowArticle, ShowBoard, ShowChat, ShowTable, ToggleSidebar,
};
use bezel::{
    gpui::{
        self, App, Context, Div, KeyBinding, Menu, MenuItem, OsAction, Window, actions, prelude::*,
    },
    ui::input,
};

actions!(
    cydonia,
    [
        CloseWindow,
        Hide,
        HideOthers,
        Minimize,
        Quit,
        ShowAll,
        ToggleFullScreen,
        Zoom
    ]
);

pub fn init(cx: &mut App) {
    // A nib gives an app these; there is no nib here, and an item whose action
    // nothing has bound shows no shortcut and answers to none — ⌘Q included.
    cx.bind_keys([
        KeyBinding::new("cmd-q", Quit, None),
        KeyBinding::new("cmd-h", Hide, None),
        KeyBinding::new("alt-cmd-h", HideOthers, None),
        KeyBinding::new("cmd-w", CloseWindow, None),
        KeyBinding::new("cmd-m", Minimize, None),
        KeyBinding::new("ctrl-cmd-f", ToggleFullScreen, None),
    ]);

    cx.on_action(|_: &Quit, cx: &mut App| cx.quit());
    cx.on_action(|_: &Hide, cx: &mut App| cx.hide());
    cx.on_action(|_: &HideOthers, cx: &mut App| cx.hide_other_apps());
    cx.on_action(|_: &ShowAll, cx: &mut App| cx.unhide_other_apps());
    cx.on_action(|_: &Minimize, cx: &mut App| front(cx, |window| window.minimize_window()));
    cx.on_action(|_: &Zoom, cx: &mut App| front(cx, |window| window.zoom_window()));
    cx.on_action(|_: &ToggleFullScreen, cx: &mut App| {
        front(cx, |window| window.toggle_fullscreen())
    });
    // The workspace goes with its window, and the running agents with it. What
    // they were saying is on disk and resumable, which is what makes this the
    // same door as ⌘Q rather than a smaller one — and the Dock opens it again.
    cx.on_action(|_: &CloseWindow, cx: &mut App| front(cx, |window| window.remove_window()));

    // Both are handled in the workspace window as well. A global handler runs
    // only once every element in the focused path has declined, so these are
    // what answers when the window in front is the settings one.
    cx.on_action(|_: &OpenProject, cx: &mut App| {
        workspace(cx, |this, window, cx| {
            this.open_project_action(&OpenProject, window, cx)
        })
    });
    cx.on_action(|_: &OpenSettings, cx: &mut App| {
        workspace(cx, |this, window, cx| {
            this.open_settings_action(&OpenSettings, window, cx)
        })
    });

    cx.set_menus(menus());
}

/// The tree.
///
/// No About: an about panel is a window this app does not have, and an item
/// that opens a web page in its place is not one.
fn menus() -> Vec<Menu> {
    vec![
        // Titled for the unbundled binary alone — a bundle takes the first
        // menu's name from `CFBundleName`, which is this same lowercase word.
        Menu::new("cydonia").items([
            MenuItem::action("Settings…", OpenSettings),
            MenuItem::separator(),
            MenuItem::action("Hide cydonia", Hide),
            MenuItem::action("Hide Others", HideOthers),
            MenuItem::action("Show All", ShowAll),
            MenuItem::separator(),
            MenuItem::action("Quit cydonia", Quit),
        ]),
        Menu::new("File").items([
            MenuItem::action("New Session", NewSession),
            MenuItem::action("New Board", NewBoard),
            MenuItem::action("New Article", NewArticle),
            MenuItem::action("New Table", NewTable),
            MenuItem::separator(),
            MenuItem::action("Open Project…", OpenProject),
            MenuItem::action("Close Project", CloseProject),
            MenuItem::separator(),
            MenuItem::action("Close Window", CloseWindow),
        ]),
        // The text field's actions, which the article's editor does not answer
        // to: it keeps a vocabulary of its own that this crate cannot name. Its
        // ⌘C is its own and reaches it, because macOS leaves a greyed item's
        // key equivalent alone — the item beside it is what goes dim.
        //
        // `os_action` puts them on the responder chain under the names AppKit
        // knows, so cut and paste work in the open panel a project is picked
        // from as well as in our own fields.
        Menu::new("Edit").items([
            MenuItem::os_action("Undo", input::Undo, OsAction::Undo),
            MenuItem::os_action("Redo", input::Redo, OsAction::Redo),
            MenuItem::separator(),
            MenuItem::os_action("Cut", input::Cut, OsAction::Cut),
            MenuItem::os_action("Copy", input::Copy, OsAction::Copy),
            MenuItem::os_action("Paste", input::Paste, OsAction::Paste),
            MenuItem::separator(),
            MenuItem::os_action("Select All", input::SelectAll, OsAction::SelectAll),
        ]),
        Menu::new("View").items([
            MenuItem::action("Toggle Sidebar", ToggleSidebar),
            MenuItem::separator(),
            MenuItem::action("Chat", ShowChat),
            MenuItem::action("Board", ShowBoard),
            MenuItem::action("Article", ShowArticle),
            MenuItem::action("Table", ShowTable),
            MenuItem::separator(),
            // Drawn ⌥⌘→ and ⌥⌘←, which is why those are bound first: the
            // `ctrl-tab` pair these also answer to is a chord gpui cannot
            // hand macOS, and an item that named it would teach ⌃T.
            MenuItem::action("Next Entry", NextEntry),
            MenuItem::action("Previous Entry", PrevEntry),
            MenuItem::separator(),
            MenuItem::action("Enter Full Screen", ToggleFullScreen),
        ]),
        // Named exactly this: `setWindowsMenu:` is hung off the title, and it
        // is what appends the window list under whatever we put in it.
        Menu::new("Window").items([
            MenuItem::action("Minimize", Minimize),
            MenuItem::action("Zoom", Zoom),
        ]),
    ]
}

/// Run `f` on the window in front. A window command means whichever window
/// that is: ⌘M over settings minimises settings.
fn front(cx: &mut App, f: impl FnOnce(&mut Window)) {
    if let Some(window) = cx.active_window() {
        let _ = window.update(cx, |_, window, _| f(window));
    }
}

/// Run `f` on the workspace window, brought forward first. Looked up rather
/// than held: ⌘W closes that window and the Dock opens another, so a handle
/// taken at launch outlives the window it names.
fn workspace(cx: &mut App, f: impl FnOnce(&mut Cydonia, &mut Window, &mut Context<Cydonia>)) {
    let Some(handle) = cx
        .windows()
        .into_iter()
        .find_map(|window| window.downcast::<Cydonia>())
    else {
        return;
    };
    let _ = handle.update(cx, |this, window, cx| {
        window.activate_window();
        f(this, window, cx);
    });
}

impl Cydonia {
    /// Hang the menu's commands on the root, each under the condition that
    /// makes it mean something — a board cannot be started in a window with no
    /// project open, so with none there is nothing here to handle `NewBoard`
    /// and macOS greys the item.
    ///
    /// The window's own editing actions are not here: they are bound inside a
    /// key context, no menu names them, and they are wanted for as long as the
    /// field they belong to is up.
    pub(crate) fn commands(&self, root: Div, cx: &mut Context<Self>) -> Div {
        let workspace = self.workspace.read(cx);
        let features = &workspace.settings.features;
        let (sessions, boards, tables) = (features.sessions, features.boards, features.tables);
        let project = workspace.active.is_some();
        let panes = [Pane::Chat, Pane::Board, Pane::Article, Pane::Table]
            .map(|pane| self.has_pane(pane, cx));
        // Nothing on screen is nothing to step from — the launch view is not
        // an entry, and its neighbour is not another one.
        let entries = self.showing(cx).is_some();

        root.on_action(cx.listener(Self::toggle_sidebar_action))
            .on_action(cx.listener(Self::open_project_action))
            .on_action(cx.listener(Self::open_settings_action))
            .when(project, |root| {
                root.on_action(cx.listener(Self::close_project_action))
                    .on_action(cx.listener(Self::new_article_action))
                    .when(sessions, |root| {
                        root.on_action(cx.listener(Self::new_session_action))
                    })
                    .when(boards, |root| {
                        root.on_action(cx.listener(Self::new_board_action))
                    })
                    .when(tables, |root| {
                        root.on_action(cx.listener(Self::new_table_action))
                    })
            })
            .when(panes[0], |root| {
                root.on_action(cx.listener(Self::show_chat))
            })
            .when(panes[1], |root| {
                root.on_action(cx.listener(Self::show_board))
            })
            .when(panes[2], |root| {
                root.on_action(cx.listener(Self::show_article))
            })
            .when(panes[3], |root| {
                root.on_action(cx.listener(Self::show_table))
            })
            .when(entries, |root| {
                root.on_action(cx.listener(Self::next_entry))
                    .on_action(cx.listener(Self::prev_entry))
            })
    }
}
