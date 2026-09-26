//! Opening an entry while an arrangement is up: the window leaves the space
//! and lands on what was picked.
//!
//! The pane a space leaves behind is the one it was focused on, and the pane
//! kind is written through `leaf_mut` — an index into the leaves that
//! `sync_leaves` is about to rewrite. These run that reconciliation by hand,
//! the way a frame would.

use super::*;
use crate::model::{settings::Settings, state};
use artifact::space::Side;
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
                    workspace.member_of(0, Showing::Board(0)).expect("a member"),
                    workspace.member_of(0, Showing::Board(1)).expect("a member"),
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
            root.open_board(0, 2, window, cx);

            assert!(
                root.workspace.read(cx).active_space().is_none(),
                "the space is left"
            );
            // And the frame after it.
            root.sync_leaves(window, cx);
            assert_eq!(root.leaves.len(), 1, "one pane");
            assert_eq!(root.leaf().pane, Pane::Board);
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
        // A new article is given a cover, which goes through the image cache.
        crate::memory::init(0, cx);
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
                    workspace.member_of(0, Showing::Board(0)).expect("a member"),
                    workspace.member_of(0, Showing::Board(1)).expect("a member"),
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
            assert_eq!(root.leaf().pane, Pane::Article);
        })
        .unwrap();
}

/// A new session leaves the space too — the case a member cannot cover, since
/// a session has no file until its first turn and so is in no arrangement by
/// name. Without this the window stays arranged and the session just started
/// is nowhere on screen.
#[gpui::test]
fn starting_a_session_inside_a_space_lands_on_it(cx: &mut gpui::TestAppContext) {
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
                    workspace.member_of(0, Showing::Board(0)).expect("a member"),
                    workspace.member_of(0, Showing::Board(1)).expect("a member"),
                )
            });
            root.workspace.update(cx, |workspace, cx| {
                workspace.arrange(&a, &b, Side::Right, cx)
            });
            root.sync_leaves(window, cx);
            root.focus_pane(&b, window, cx);

            root.pick_agent(0, window, cx);

            assert!(
                root.workspace.read(cx).active_space().is_none(),
                "the space is left"
            );
            root.sync_leaves(window, cx);
            assert_eq!(root.leaf().pane, Pane::Chat);
            assert!(
                root.workspace.read(cx).active_id().is_some(),
                "on a session"
            );
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
                    workspace.member_of(0, Showing::Board(0)).expect("a member"),
                    workspace.member_of(0, Showing::Board(1)).expect("a member"),
                )
            });
            root.workspace.update(cx, |workspace, cx| {
                workspace.arrange(&a, &b, Side::Right, cx)
            });
            root.sync_leaves(window, cx);

            // Out of the space, onto a board no space holds.
            root.open_board(0, 2, window, cx);
            root.sync_leaves(window, cx);
            assert!(root.workspace.read(cx).active_space().is_none(), "left it");

            // And back to one the space does hold, from outside it.
            root.open_board(0, 1, window, cx);
            root.sync_leaves(window, cx);
            assert_eq!(
                root.leaf().entry.as_ref(),
                Some(&b),
                "the second pane, which is the one picked"
            );

            // Out again, and back onto the *first* pane this time: a miss in
            // `focus_pane` clamps to the last leaf, so picking the last pane
            // cannot tell a hit from a miss.
            root.open_board(0, 2, window, cx);
            root.sync_leaves(window, cx);
            root.open_board(0, 0, window, cx);

            assert!(
                root.workspace.read(cx).active_space().is_some(),
                "the arrangement holding it is what opening it opens"
            );
            root.sync_leaves(window, cx);
            assert_eq!(root.leaves.len(), 2, "arranged, not alone");
            assert_eq!(
                root.leaf().entry.as_ref(),
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
                let board = workspace.member_of(0, Showing::Board(0)).expect("a member");

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
                workspace.projects[0].sessions.push(chat);
                let session = workspace.member_of_session(7).expect("a member");
                (board, session)
            });

            // The session second, so it is the pane the old focus would have
            // been dragged to.
            root.workspace.update(cx, |workspace, cx| {
                workspace.arrange(&board, &session, Side::Right, cx)
            });
            root.sync_leaves(window, cx);
            root.focus_pane(&board, window, cx);
            assert_eq!(root.leaf().entry.as_ref(), Some(&board), "on the board");

            root.sync_composer(cx);
            board
        })
        .unwrap();

    // The composer's event is emitted, not delivered: the subscription runs
    // when the effects flush, which is after the frame that synced.
    cx.run_until_parked();

    window
        .update(cx, |root, _, _| {
            assert_eq!(
                root.leaf().entry.as_ref(),
                Some(&board),
                "and still on the board after the sync"
            );
        })
        .unwrap();
}
