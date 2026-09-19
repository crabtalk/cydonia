//! The macOS menu bar: the tree, the commands only it names, and the wiring
//! that decides which of them are live.
//!
//! An item carries an action and a name, never a shortcut. `set_menus` reads
//! the equivalent off the keymap, so [`crate::view::keymap`] stays the one
//! place a chord is written — at the price of two rules:
//!
//! * the keymap is bound before this module runs, or an item is built for an
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

use crate::{
    model::{settings, update},
    view::{
        article::TogglePlainText,
        leaf::Pane,
        root::{
            CloseProject, Cydonia, NewArticle, NewBoard, NewSession, NewSessionNext,
            NewSessionWith, NewTable, NextEntry, OpenProject, OpenSettings, PrevEntry,
            ToggleChanges, ToggleSidebar, ToggleTerminal,
        },
    },
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
        CheckForUpdates,
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

/// The window and application chords, which are not the reader's to move: they
/// are where macOS puts them for every app, and an app that let you move them
/// would be the only one you had to remember. Bound at all because a nib gives
/// an app these and there is no nib here — an item whose action nothing has
/// bound shows no shortcut and answers to none, ⌘Q included.
pub fn bindings() -> Vec<KeyBinding> {
    vec![
        KeyBinding::new("cmd-q", Quit, None),
        KeyBinding::new("cmd-h", Hide, None),
        KeyBinding::new("alt-cmd-h", HideOthers, None),
        KeyBinding::new("cmd-w", CloseWindow, None),
        KeyBinding::new("cmd-m", Minimize, None),
        KeyBinding::new("ctrl-cmd-f", ToggleFullScreen, None),
    ]
}

pub fn init(cx: &mut App) {
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

    // No chord: looking for a release is not something to reach for by hand
    // twice, and the app does it on its own anyway. Hung on the updater it acts
    // on, so a build that has none also has no handler and no item — see
    // [`menus`].
    if update::supported(cx)
        && let Some(updater) = update::of(cx)
    {
        cx.on_action(move |_: &CheckForUpdates, cx: &mut App| {
            updater.update(cx, |updater, cx| updater.check(true, cx));
        });
    }

    refresh(cx);
}

/// Build the tree again and hand it over, so every item carries the chord the
/// keymap holds *now*, and File names the agents installed *now*.
///
/// Called at launch, after a rebind, and after an agent is installed or
/// removed. The rebind is the one that has to be: AppKit keeps the key
/// equivalent it was given and claims the chord before gpui sees it, so a
/// moved shortcut that left the menus alone would leave the old chord firing.
pub fn refresh(cx: &mut App) {
    let menus = menus(cx);
    cx.set_menus(menus);
}

/// The tree.
///
/// No About: an about panel is a window this app does not have, and an item
/// that opens a web page in its place is not one.
fn menus(cx: &App) -> Vec<Menu> {
    // The one conditional item in the tree, and it leads the app menu the way
    // it does in every other mac app. A build that cannot replace itself — a
    // `cargo install` binary, a working copy, an architecture no image is cut
    // for — gets no item rather than one that would decline.
    let mut app = Vec::new();
    if update::supported(cx) {
        app.push(MenuItem::action("Check for Updates…", CheckForUpdates));
        app.push(MenuItem::separator());
    }
    app.extend([
        MenuItem::action("Settings…", OpenSettings),
        MenuItem::separator(),
        MenuItem::action("Hide cydonia", Hide),
        MenuItem::action("Hide Others", HideOthers),
        MenuItem::action("Show All", ShowAll),
        MenuItem::separator(),
        MenuItem::action("Quit cydonia", Quit),
    ]);
    let file = file_menu();
    vec![
        // Titled for the unbundled binary alone — a bundle takes the first
        // menu's name from `CFBundleName`, which is this same lowercase word.
        Menu::new("cydonia").items(app),
        Menu::new("File").items(file),
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
            MenuItem::action("Toggle Terminal", ToggleTerminal),
            MenuItem::action("Toggle Right Panel", ToggleChanges),
            MenuItem::action("Open Files", crate::view::root::OpenFiles),
            MenuItem::action("Open Review", crate::view::root::OpenReview),
            MenuItem::separator(),
            // No chord drawn beside either: ⌃⇥ and ⇧⌃⇥ are what step between
            // entries, and gpui has no macOS equivalent for `tab` — an item
            // naming that pair would teach ⌃T. Any other chord put here would
            // be claimed by AppKit ahead of the window, which is what left the
            // pair that used to be drawn here doing nothing at all.
            MenuItem::action("Next Entry", NextEntry),
            MenuItem::action("Previous Entry", PrevEntry),
            MenuItem::separator(),
            // Here rather than left to the pane's own `···`, because ⌘E is the
            // editor's inline code and only a key equivalent on the bar takes
            // a chord before the focused surface is offered it — see
            // [`crate::view::keymap::Command::PlainText`].
            MenuItem::action("Plain Text", TogglePlainText),
            MenuItem::action("Find Card", crate::view::board::FindCard),
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

/// File, whose session items depend on what is installed: one agent is one
/// "New Session", and a second is what brings in the rest. The agents are read
/// off `settings.toml` rather than the workspace, which at launch has no window
/// yet to be read from.
fn file_menu() -> Vec<MenuItem> {
    let agents = settings::load()
        .map(|settings| settings.agents)
        .unwrap_or_default();
    let mut file = vec![MenuItem::action("New Session", NewSession)];
    if agents.len() > 1 {
        file.push(MenuItem::submenu(Menu::new("New Session With").items(
            agents.into_iter().map(|agent| {
                MenuItem::action(agent.name.clone(), NewSessionWith { agent: agent.name })
            }),
        )));
        file.push(MenuItem::action(
            "New Session on Next Agent",
            NewSessionNext,
        ));
    }
    file.extend([
        MenuItem::separator(),
        MenuItem::action("New Board", NewBoard),
        MenuItem::action("New Article", NewArticle),
        MenuItem::action("New Table", NewTable),
        MenuItem::separator(),
        MenuItem::action("Open Project…", OpenProject),
        MenuItem::action("Close Project", CloseProject),
        MenuItem::separator(),
        MenuItem::action("Close Window", CloseWindow),
    ]);
    file
}

/// Run `f` on the window in front. A window command means whichever window
/// that is: ⌘M over settings minimises settings, and ⌘W closes it.
///
/// Left to the next turn, and that is the whole of why it arrives at all: a
/// global handler runs inside the front window's own update, which holds that
/// window out of the app while it runs, so asking for it here and now finds
/// nothing and the command is dropped on the floor. Deferred, it is back.
fn front(cx: &mut App, f: impl FnOnce(&mut Window) + 'static) {
    let Some(window) = cx.active_window() else {
        return;
    };
    cx.defer(move |cx| {
        let _ = window.update(cx, |_, window, _| f(window));
    });
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
        // Nothing on screen is nothing to step from — the launch view is not
        // an entry, and its neighbour is not another one.
        let showing = self.showing(cx);
        let entries = showing.is_some();

        root.on_action(cx.listener(Self::toggle_sidebar_action))
            .on_action(cx.listener(Self::open_project_action))
            .on_action(cx.listener(Self::open_settings_action))
            .when(project, |root| {
                root.on_action(cx.listener(Self::close_project_action))
                    .on_action(cx.listener(Self::new_article_action))
                    .when(sessions, |root| {
                        root.on_action(cx.listener(Self::new_session_action))
                            .on_action(cx.listener(Self::new_session_with_action))
                            .on_action(cx.listener(Self::new_session_next_action))
                    })
                    .when(boards, |root| {
                        root.on_action(cx.listener(Self::new_board_action))
                    })
                    .when(tables, |root| {
                        root.on_action(cx.listener(Self::new_table_action))
                    })
            })
            // The bottom panel is the window's, not a pane's: it stands under
            // whatever is showing, a layout included. A project is what it
            // needs, for the directory its first shell opens in.
            .when(project, |root| {
                root.on_action(cx.listener(Self::toggle_terminal))
            })
            // Pane-specific commands grey themselves everywhere else.
            .when(showing == Some(Pane::Article), |root| {
                root.on_action(cx.listener(Self::toggle_plain_text))
            })
            .when(showing == Some(Pane::Board), |root| {
                root.on_action(cx.listener(Self::find_card))
            })
            .when(entries, |root| {
                root.on_action(cx.listener(Self::next_entry))
                    .on_action(cx.listener(Self::prev_entry))
            })
    }
}
