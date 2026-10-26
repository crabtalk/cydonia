//! Opening an entry while an arrangement is up: the window leaves the space
//! and lands on what was picked.
//!
//! The pane a space leaves behind is the one it was focused on, and the pane
//! kind is written through `leaf_mut` — an index into the leaves that
//! `sync_leaves` is about to rewrite. These run that reconciliation by hand,
//! the way a frame would.

use super::*;
use crate::{
    model::{
        identity::Identity,
        settings::Settings,
        state,
        workspace::{Opening, Slot},
    },
    view::sidebar::{Group, Row},
};
use artifact::{kind::Kind, space::Side};
use bezel::gpui;

/// A scratch project, and a config directory beside it that the test's writes
/// land in — see the same guard in `tests/unit/spaces.rs`.
struct Scratch(std::path::PathBuf);

impl Scratch {
    fn new(name: &str) -> Self {
        let root = std::env::temp_dir().join(format!("cydonia-open-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("config")).unwrap();
        unsafe { std::env::set_var("XDG_CONFIG_HOME", root.join("config")) };
        Self(root)
    }

    fn project(&self, name: &str) -> std::path::PathBuf {
        let path = self.0.join(name);
        std::fs::create_dir_all(&path).unwrap();
        path
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Picking a board no pane is on leaves the arrangement and shows it, from
/// whichever pane the focus was in.
#[gpui::test]
fn opening_an_entry_outside_the_space_lands_on_it(cx: &mut gpui::TestAppContext) {
    let scratch = Scratch::new("outside");
    cx.update(|cx| Theme::install(bezel::theme::Appearance::Light, cx));
    let window = cx.add_window(|window, cx| {
        Cydonia::new(Settings::default(), state::State::default(), window, cx)
    });

    window
        .update(cx, |root, window, cx| {
            let (a, b) = root.workspace.update(cx, |workspace, cx| {
                workspace.open_project(scratch.project("one"), cx);
                // Newest first, so the third made is board 0 and the first
                // made is board 2.
                workspace.new_board(0, "First".into(), "ONE", cx).ok();
                workspace.new_board(0, "Second".into(), "TWO", cx).ok();
                workspace.new_board(0, "Third".into(), "THR", cx).ok();
                (
                    workspace.board_member(0, 0).expect("a member"),
                    workspace.board_member(0, 1).expect("a member"),
                )
            });
            root.workspace.update(cx, |workspace, cx| {
                workspace.arrange(&a, &b, Side::Right, cx)
            });
            // The frame the arrangement would have been drawn in.
            root.sync_leaves(window, cx);
            assert_eq!(root.leaves.len(), 2, "a pane each");
            // Focused on the second pane, which is the case the fix is about:
            // the pane kind is written through `leaf_mut`.
            root.focus_pane(&b, window, cx);
            assert_eq!(root.focused, 1);

            // The board no pane is on.
            root.open_board(0, 2, Opening::Preview, window, cx);

            assert!(
                root.workspace.read(cx).active_space().is_none(),
                "the space is left"
            );
            // And the frame after it.
            root.sync_leaves(window, cx);
            assert_eq!(root.leaves.len(), 1, "one pane");
            assert_eq!(root.showing(cx), Some(Kind::Board));
            assert_eq!(
                root.workspace
                    .read(cx)
                    .active_project()
                    .and_then(|open| open.board),
                Some(2),
                "on the board that was picked"
            );
        })
        .unwrap();
}

/// Making an entry while an arrangement is up does the same: the space is
/// left and the window lands on the thing just made, not on whichever pane
/// the arrangement had first.
#[gpui::test]
fn making_an_entry_inside_a_space_lands_on_it(cx: &mut gpui::TestAppContext) {
    let scratch = Scratch::new("making");
    cx.update(|cx| {
        Theme::install(bezel::theme::Appearance::Light, cx);
    });
    let window = cx.add_window(|window, cx| {
        Cydonia::new(Settings::default(), state::State::default(), window, cx)
    });

    window
        .update(cx, |root, window, cx| {
            let (a, b) = root.workspace.update(cx, |workspace, cx| {
                workspace.open_project(scratch.project("one"), cx);
                workspace.new_board(0, "First".into(), "ONE", cx).ok();
                workspace.new_board(0, "Second".into(), "TWO", cx).ok();
                (
                    workspace.board_member(0, 0).expect("a member"),
                    workspace.board_member(0, 1).expect("a member"),
                )
            });
            root.workspace.update(cx, |workspace, cx| {
                workspace.arrange(&a, &b, Side::Right, cx)
            });
            root.sync_leaves(window, cx);
            root.focus_pane(&b, window, cx);

            root.new_article(0, window, cx);

            assert!(
                root.workspace.read(cx).active_space().is_none(),
                "the space is left"
            );
            root.sync_leaves(window, cx);
            assert_eq!(root.showing(cx), Some(Kind::Article));
        })
        .unwrap();
}

/// A new session started inside a space stays in it, in front of the focused
/// pane's stack, though it has no file yet to be named in the arrangement by.
#[gpui::test]
fn starting_a_session_inside_a_space_lands_in_the_focused_pane(cx: &mut gpui::TestAppContext) {
    let scratch = Scratch::new("session");
    cx.update(|cx| Theme::install(bezel::theme::Appearance::Light, cx));
    let window = cx.add_window(|window, cx| {
        Cydonia::new(Settings::default(), state::State::default(), window, cx)
    });

    window
        .update(cx, |root, window, cx| {
            let (a, b) = root.workspace.update(cx, |workspace, cx| {
                workspace.settings.features.sessions = true;
                workspace.settings.agents = vec![crate::model::settings::Agent {
                    name: "test".into(),
                    id: None,
                    command: String::new(),
                    args: vec![],
                    env: Default::default(),
                }];
                workspace.open_project(scratch.project("one"), cx);
                workspace.new_board(0, "First".into(), "ONE", cx).ok();
                workspace.new_board(0, "Second".into(), "TWO", cx).ok();
                (
                    workspace.board_member(0, 0).expect("a member"),
                    workspace.board_member(0, 1).expect("a member"),
                )
            });
            root.workspace.update(cx, |workspace, cx| {
                workspace.arrange(&a, &b, Side::Right, cx)
            });
            root.sync_leaves(window, cx);
            root.focus_pane(&b, window, cx);

            root.pick_agent(0, window, cx);

            assert!(
                root.workspace.read(cx).active_space().is_some(),
                "the space stays"
            );
            root.sync_leaves(window, cx);
            assert_eq!(root.showing(cx), Some(Kind::Session));
            let session = root.focused_entry(cx).expect("a focused pane");
            let stack = root.workspace.read(cx).stack_of(&session);
            assert!(stack.contains(&b), "stacked over the focused pane");
            assert!(!stack.contains(&a), "not over the other one");
        })
        .unwrap();
}

/// Picking an entry a space holds goes to that space's pane, from a window
/// that is not in the space at all.
///
/// The gesture means one thing wherever it is made: the sidebar lists an entry
/// under the space holding it or under its project, never both, and that one
/// place is what opening it goes to — see [`Cydonia::enter_member`].
#[gpui::test]
fn opening_an_entry_a_space_holds_enters_the_space(cx: &mut gpui::TestAppContext) {
    let scratch = Scratch::new("enter");
    cx.update(|cx| Theme::install(bezel::theme::Appearance::Light, cx));
    let window = cx.add_window(|window, cx| {
        Cydonia::new(Settings::default(), state::State::default(), window, cx)
    });

    window
        .update(cx, |root, window, cx| {
            // Newest first, so the third made is board 0 and the first is 2.
            let (a, b) = root.workspace.update(cx, |workspace, cx| {
                workspace.open_project(scratch.project("one"), cx);
                workspace.new_board(0, "First".into(), "ONE", cx).ok();
                workspace.new_board(0, "Second".into(), "TWO", cx).ok();
                workspace.new_board(0, "Third".into(), "THR", cx).ok();
                (
                    workspace.board_member(0, 0).expect("a member"),
                    workspace.board_member(0, 1).expect("a member"),
                )
            });
            root.workspace.update(cx, |workspace, cx| {
                workspace.arrange(&a, &b, Side::Right, cx)
            });
            root.sync_leaves(window, cx);

            // Out of the space, onto a board no space holds.
            root.open_board(0, 2, Opening::Preview, window, cx);
            root.sync_leaves(window, cx);
            assert!(root.workspace.read(cx).active_space().is_none(), "left it");

            // And back to one the space does hold, from outside it.
            root.open_board(0, 1, Opening::Preview, window, cx);
            root.sync_leaves(window, cx);
            assert_eq!(
                Some(&root.leaf().unwrap().read(cx).entry),
                Some(&b),
                "the second pane, which is the one picked"
            );

            // Out again, and back onto the *first* pane this time: a miss in
            // `focus_pane` clamps to the last leaf, so picking the last pane
            // cannot tell a hit from a miss.
            root.open_board(0, 2, Opening::Preview, window, cx);
            root.sync_leaves(window, cx);
            root.open_board(0, 0, Opening::Preview, window, cx);

            assert!(
                root.workspace.read(cx).active_space().is_some(),
                "the arrangement holding it is what opening it opens"
            );
            root.sync_leaves(window, cx);
            assert_eq!(root.panes.len(), 2, "arranged, not alone");
            assert_eq!(
                Some(&root.leaf().unwrap().read(cx).entry),
                Some(&a),
                "focused on the pane that was picked"
            );
        })
        .unwrap();
}

/// A session pane does not pull the focus to itself as the frame syncs.
///
/// `sync_composer` writes the draft into every pane's composer, which the
/// field reports as a change and the composer as a draft. Moving the focus on
/// that put it on the last session in the arrangement — whatever pane was
/// picked — because the sync runs over the leaves in order.
#[gpui::test]
fn syncing_the_composers_leaves_the_focus_where_it_was(cx: &mut gpui::TestAppContext) {
    let scratch = Scratch::new("sync-composer");
    cx.update(|cx| Theme::install(bezel::theme::Appearance::Light, cx));
    let mut settings = Settings::default();
    settings.features.sessions = true;
    let window =
        cx.add_window(|window, cx| Cydonia::new(settings, state::State::default(), window, cx));

    let board = window
        .update(cx, |root, window, cx| {
            let (board, session) = root.workspace.update(cx, |workspace, cx| {
                workspace.open_project(scratch.project("one"), cx);
                workspace.new_board(0, "First".into(), "ONE", cx).ok();
                let board = workspace.board_member(0, 0).expect("a member");

                let path = workspace.projects[0].path.clone();
                let mut chat = crate::model::session::ChatSession::restore(
                    7,
                    path,
                    crate::model::settings::Agent {
                        name: "test".into(),
                        id: None,
                        command: String::new(),
                        args: Vec::new(),
                        env: Default::default(),
                    },
                    serde_json::from_value(serde_json::json!({
                        "id": "test", "agent": "test", "title": "", "name": null,
                        "updated": 1, "items": []
                    }))
                    .expect("a record"),
                );
                // A draft to write in, which is what the sync does to the
                // field and what the field reports as a change.
                chat.draft = "half a thought".into();
                workspace.projects[0]
                    .sessions
                    .push(crate::model::project::HeldSession::new(chat, cx));
                let session = workspace.member_of_session(7, cx).expect("a member");
                (board, session)
            });

            // The session second, so it is the pane the old focus would have
            // been dragged to.
            root.workspace.update(cx, |workspace, cx| {
                workspace.arrange(&board, &session, Side::Right, cx)
            });
            root.sync_leaves(window, cx);
            root.focus_pane(&board, window, cx);
            assert_eq!(
                Some(&root.leaf().unwrap().read(cx).entry),
                Some(&board),
                "on the board"
            );

            root.sync_composer(cx);
            board
        })
        .unwrap();

    // The composer's event is emitted, not delivered: the subscription runs
    // when the effects flush, which is after the frame that synced.
    cx.run_until_parked();

    window
        .update(cx, |root, _, cx| {
            assert_eq!(
                Some(&root.leaf().unwrap().read(cx).entry),
                Some(&board),
                "and still on the board after the sync"
            );
        })
        .unwrap();
}

/// An agent's heading carried onto a pane lands as a new session started as
/// that agent, in its workspace, made only once it lands.
#[gpui::test]
fn an_agent_lands_as_a_session_started_as_it(cx: &mut gpui::TestAppContext) {
    let scratch = Scratch::new("agent");
    cx.update(|cx| Theme::install(bezel::theme::Appearance::Light, cx));
    let window = cx.add_window(|window, cx| {
        Cydonia::new(Settings::default(), state::State::default(), window, cx)
    });

    window
        .update(cx, |root, _, cx| {
            let project = scratch.project("one");
            let id = root.workspace.update(cx, |workspace, cx| {
                workspace.settings.features.sessions = true;
                workspace.settings.agents = vec![crate::model::settings::Agent {
                    name: "test".into(),
                    id: Some("test-acp".into()),
                    command: String::new(),
                    args: vec![],
                    env: Default::default(),
                }];
                let identity =
                    Identity::new("Alice", project.clone(), "test-acp", Default::default());
                workspace.create_identity(identity, cx).expect("kept")
            });
            let sessions = |cx: &App| root.workspace.read(cx).identity_sessions(&id, cx).len();
            assert_eq!(sessions(cx), 0, "nothing before the drop");

            let landed = Row::Group(Group::Agent(id.clone())).landed(&root.workspace, cx);

            assert!(landed.is_some(), "lands as a member");
            assert_eq!(sessions(cx), 1, "one session, started as the agent");
            let workspace = root.workspace.read(cx);
            assert_eq!(workspace.projects[0].path, project, "in its workspace");
        })
        .unwrap();
}

/// An archived entry leaves the sidebar's list; the library is what lists it.
#[gpui::test]
fn an_archived_entry_leaves_the_sidebar(cx: &mut gpui::TestAppContext) {
    let scratch = Scratch::new("archived");
    cx.update(|cx| Theme::install(bezel::theme::Appearance::Light, cx));
    let window = cx.add_window(|window, cx| {
        Cydonia::new(Settings::default(), state::State::default(), window, cx)
    });

    window
        .update(cx, |root, _, cx| {
            let (kept, put_away) = root.workspace.update(cx, |workspace, cx| {
                workspace.open_project(scratch.project("one"), cx);
                workspace.new_board(0, "Kept".into(), "ONE", cx).ok();
                workspace.new_board(0, "Put away".into(), "TWO", cx).ok();
                let id = workspace.projects[0].boards[1].id.clone();
                let row = |at| Row::of_member(&workspace.board_member(0, at)?, workspace, cx);
                let rows = (row(0), row(1));
                workspace.archive_board(&id, true, cx);
                rows
            });
            let rows = root.list(cx).rows();
            assert!(rows.contains(&kept.expect("a row")), "in use stays");
            assert!(!rows.contains(&put_away.expect("a row")), "archived leaves");
        })
        .unwrap();
}

/// The library opens as a tab of the strip: an entry opened over it leaves the
/// tab where it is, bringing it back shows the library again, and closing it
/// takes the tab off.
#[gpui::test]
fn the_library_is_a_tab_of_the_strip(cx: &mut gpui::TestAppContext) {
    let scratch = Scratch::new("library");
    cx.update(|cx| Theme::install(bezel::theme::Appearance::Light, cx));
    let window = cx.add_window(|window, cx| {
        Cydonia::new(Settings::default(), state::State::default(), window, cx)
    });

    window
        .update(cx, |root, window, cx| {
            root.workspace.update(cx, |workspace, cx| {
                workspace.open_project(scratch.project("one"), cx);
                workspace.new_board(0, "First".into(), "ONE", cx).ok();
            });
            let library = Slot::Library {};
            let held =
                |root: &Cydonia, cx: &App| root.workspace.read(cx).strip().contains(&library);

            root.toggle_library(window, cx);
            assert!(held(root, cx) && root.library_shown(cx), "opened in front");

            root.open_board(0, 0, Opening::Kept, window, cx);
            assert!(held(root, cx), "the tab stays");
            assert!(!root.library_shown(cx), "the board is in front");

            root.show_slot(&library, window, cx);
            assert!(root.library_shown(cx), "brought back");

            root.close_library(window, cx);
            assert!(!held(root, cx), "closed");
            assert!(root.library.is_none(), "nothing held for it");
        })
        .unwrap();
}
