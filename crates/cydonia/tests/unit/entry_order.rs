//! The sidebar order: what it is keyed by, what a new entry does to it, and
//! what survives a relaunch.
//!
//! The sort itself is the sidebar's — see [`crate::view::sidebar::ranked`].
//! What is checked here is the rank it reads, which is the part that is
//! written down.

use super::*;
use crate::model::workspace::Showing;
use crate::model::{settings::Settings, state};
use gpui::AppContext as _;

/// A scratch project and a config directory of its own, for the same reason
/// [`super::super::layouts`]'s tests have one: [`Workspace::save`] writes
/// `state.toml` for real, and a test must not rewrite the project list of
/// whoever ran it.
struct Scratch(std::path::PathBuf);

impl Scratch {
    fn new(name: &str) -> Self {
        let root = std::env::temp_dir().join(format!("cydonia-order-{name}-{}", std::process::id()));
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

/// A workspace on one scratch project holding three boards.
fn three_boards(
    scratch: &Scratch,
    cx: &mut gpui::TestAppContext,
) -> gpui::Entity<crate::model::workspace::Workspace> {
    cx.update(|cx| bezel::theme::Theme::install(bezel::theme::Appearance::Light, cx));
    let workspace = cx.new(|cx| Workspace::new(Settings::default(), state::State::default(), cx));
    workspace.update(cx, |workspace, cx| {
        workspace.open_project(scratch.project("one"), cx);
        workspace.new_board(0, "First".into(), "ONE", cx).ok();
        workspace.new_board(0, "Second".into(), "TWO", cx).ok();
        workspace.new_board(0, "Third".into(), "THR", cx).ok();
    });
    workspace
}

/// The order is held by what identifies an entry, not by where it sits: a
/// board made or dropped beside one it names must not move it.
#[gpui::test]
fn the_order_is_keyed_by_identity(cx: &mut gpui::TestAppContext) {
    let scratch = Scratch::new("identity");
    let workspace = three_boards(&scratch, cx);

    workspace.update(cx, |workspace, cx| {
        let entries: Vec<state::Entry> = (0..3)
            .map(|ix| workspace.entry_of(0, Showing::Board(ix)).expect("an entry"))
            .collect();
        // Back to front, which is the arrangement no stamp would produce.
        let reversed: Vec<state::Entry> = entries.iter().rev().cloned().collect();
        workspace.set_order(0, reversed, cx);

        assert_eq!(workspace.rank_of(0, Showing::Board(0)), Some(2));
        assert_eq!(workspace.rank_of(0, Showing::Board(2)), Some(0));
    });
}

/// An entry made since the order was written has no rank — the sidebar lists
/// those above the arrangement, so a new entry is never born under the fold.
#[gpui::test]
fn a_new_entry_has_no_rank(cx: &mut gpui::TestAppContext) {
    let scratch = Scratch::new("new");
    let workspace = three_boards(&scratch, cx);

    workspace.update(cx, |workspace, cx| {
        let held: Vec<state::Entry> = (0..3)
            .map(|ix| workspace.entry_of(0, Showing::Board(ix)).expect("an entry"))
            .collect();
        workspace.set_order(0, held, cx);
        workspace.new_board(0, "Fourth".into(), "FOU", cx).ok();

        let fresh = (0..4)
            .find(|ix| workspace.rank_of(0, Showing::Board(*ix)).is_none())
            .expect("the new one is unranked");
        assert_eq!(
            workspace.rank_of(0, Showing::Board(fresh)),
            None,
            "made since the order was written"
        );
    });
}

/// The arrangement is bookkeeping, so it goes in `state.toml` and comes back
/// out of it — a relaunch lists the entries where they were left.
#[gpui::test]
fn the_order_survives_a_relaunch(cx: &mut gpui::TestAppContext) {
    let scratch = Scratch::new("relaunch");
    let workspace = three_boards(&scratch, cx);
    let path = scratch.project("one");

    workspace.update(cx, |workspace, cx| {
        let reversed: Vec<state::Entry> = (0..3)
            .rev()
            .map(|ix| workspace.entry_of(0, Showing::Board(ix)).expect("an entry"))
            .collect();
        workspace.set_order(0, reversed, cx);
    });

    let restored = state::restore();
    let held = restored.order.get(&path).expect("written down");
    assert_eq!(held.len(), 3, "every entry, not just the one that moved");

    // And read back into a workspace that never saw the drag.
    let next = cx.new(|cx| Workspace::new(Settings::default(), restored, cx));
    next.update(cx, |workspace, _| {
        assert_eq!(workspace.rank_of(0, Showing::Board(0)), Some(2));
    });
}
