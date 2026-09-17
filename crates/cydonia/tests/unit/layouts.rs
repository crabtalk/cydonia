//! Layouts are born from a drag, not from a menu: the first entry dropped on
//! the one already open mints one, and every drag after it lands in the same
//! layout.

use super::*;
use crate::model::workspace::Showing;
use crate::model::{settings::Settings, state};
use artifact::layout::Side;
use gpui::AppContext as _;

/// A scratch project, torn down when the test ends.
struct Scratch(std::path::PathBuf);

impl Scratch {
    fn new(name: &str) -> Self {
        let path =
            std::env::temp_dir().join(format!("cydonia-layouts-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).unwrap();
        Self(path)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Open a workspace on one scratch project.
fn opened(scratch: &Scratch, cx: &mut gpui::TestAppContext) -> gpui::Entity<Workspace> {
    cx.update(|cx| bezel::theme::Theme::install(bezel::theme::Appearance::Light, cx));
    let workspace = cx.new(|cx| Workspace::new(Settings::default(), state::State::default(), cx));
    workspace.update(cx, |workspace, cx| {
        workspace.open_project(scratch.0.clone(), cx);
    });
    workspace
}

/// The first drag mints the layout over the pane that was already there, and
/// names it `layout-1`.
#[gpui::test]
fn the_first_drag_mints_a_layout(cx: &mut gpui::TestAppContext) {
    let scratch = Scratch::new("first");
    let workspace = opened(&scratch, cx);

    workspace.update(cx, |workspace, cx| {
        assert!(workspace.projects[0].layouts.is_empty(), "none to start");

        let at = workspace.arrange(0, 1, 2, Side::Right, cx);
        assert_eq!(at, Some(0));

        let layout = workspace.active_layout().expect("open");
        assert_eq!(layout.name, "layout-1");
        assert_eq!(layout.entries(), vec![1, 2], "the pane it was made over");
    });
}

/// Every drag after the first lands in the same layout rather than minting
/// another — a week of dragging must not leave a sidebar of them.
#[gpui::test]
fn later_drags_land_in_the_same_layout(cx: &mut gpui::TestAppContext) {
    let scratch = Scratch::new("same");
    let workspace = opened(&scratch, cx);

    workspace.update(cx, |workspace, cx| {
        workspace.arrange(0, 1, 2, Side::Right, cx);
        workspace.arrange(0, 2, 3, Side::Below, cx);
        workspace.arrange(0, 3, 4, Side::Right, cx);

        assert_eq!(workspace.projects[0].layouts.len(), 1, "one layout");
        let layout = workspace.active_layout().expect("open");
        assert_eq!(layout.leaves(), 4);
    });
}

/// A layout is written as it is made, so the arrangement is on disk before the
/// window closes.
#[gpui::test]
fn a_layout_is_written_as_it_is_arranged(cx: &mut gpui::TestAppContext) {
    let scratch = Scratch::new("written");
    let workspace = opened(&scratch, cx);

    let id = workspace.update(cx, |workspace, cx| {
        workspace.arrange(0, 1, 2, Side::Right, cx);
        workspace.active_layout().expect("open").id.clone()
    });

    let store = fs::Project::new(&scratch.0);
    let read = store.layout(&id).expect("on disk");
    assert_eq!(read.entries(), vec![1, 2]);
}

/// Closing a pane while others remain leaves the layout arranging the rest.
#[gpui::test]
fn closing_one_of_several_leaves_the_layout(cx: &mut gpui::TestAppContext) {
    let scratch = Scratch::new("closing");
    let workspace = opened(&scratch, cx);

    workspace.update(cx, |workspace, cx| {
        workspace.arrange(0, 1, 2, Side::Right, cx);
        workspace.arrange(0, 2, 3, Side::Below, cx);
        workspace.close_pane(3, cx);

        let layout = workspace.active_layout().expect("still open");
        assert_eq!(layout.entries(), vec![1, 2]);
        assert_eq!(workspace.projects[0].layouts.len(), 1);
    });
}

/// Deleting a layout leaves the entries it arranged alone — it held none of
/// them.
#[gpui::test]
fn deleting_a_layout_leaves_its_members(cx: &mut gpui::TestAppContext) {
    let scratch = Scratch::new("delete");
    let workspace = opened(&scratch, cx);

    workspace.update(cx, |workspace, cx| {
        let board = workspace.new_board(0, "Roadmap".into(), "ROAD", cx).ok();
        assert!(board.is_some());
        let boards = workspace.projects[0].boards.len();

        workspace.arrange(0, 1, 2, Side::Right, cx);
        workspace.delete_layout(0, 0, cx);

        assert!(workspace.projects[0].layouts.is_empty());
        assert_eq!(workspace.projects[0].boards.len(), boards, "boards stay");
    });
}

/// A renamed layout keeps its name across a re-read.
#[gpui::test]
fn a_rename_is_written(cx: &mut gpui::TestAppContext) {
    let scratch = Scratch::new("rename");
    let workspace = opened(&scratch, cx);

    let id = workspace.update(cx, |workspace, cx| {
        workspace.arrange(0, 1, 2, Side::Right, cx);
        let id = workspace.active_layout().expect("open").id.clone();
        workspace.rename_layout(&id, "Auth work".into(), cx);
        id
    });

    let read = fs::Project::new(&scratch.0).layout(&id).expect("on disk");
    assert_eq!(read.name, "Auth work");
}

/// The next layout's name climbs past the highest taken, so a name a deleted
/// layout held is not handed to a new one.
#[gpui::test]
fn names_do_not_come_back(cx: &mut gpui::TestAppContext) {
    let scratch = Scratch::new("names");
    let workspace = opened(&scratch, cx);

    workspace.update(cx, |workspace, cx| {
        workspace.arrange(0, 1, 2, Side::Right, cx);
        workspace.projects[0].layout = None;
        workspace.arrange(0, 3, 4, Side::Right, cx);

        let names: Vec<String> = workspace.projects[0]
            .layouts
            .iter()
            .map(|layout| layout.name.clone())
            .collect();
        assert!(names.contains(&"layout-1".to_owned()), "{names:?}");
        assert!(names.contains(&"layout-2".to_owned()), "{names:?}");
    });
}

/// A member whose entry has gone leaves the layout, so no pane is left
/// pointing at a number nothing answers to.
#[gpui::test]
fn a_member_that_has_gone_is_pruned(cx: &mut gpui::TestAppContext) {
    let scratch = Scratch::new("prune");
    let workspace = opened(&scratch, cx);

    workspace.update(cx, |workspace, cx| {
        let board = workspace.projects[0]
            .store()
            .create_board("Roadmap", "ROAD")
            .expect("a board");
        let number = artifact::entry::number(&scratch.0, "board", &board.id).expect("numbered");

        // A number that was never given to anything stands for one that has
        // since been deleted: neither resolves.
        workspace.arrange(0, number, number + 500, Side::Right, cx);
        assert_eq!(workspace.active_layout().expect("open").leaves(), 2);

        workspace.prune_layouts(cx);
        let layout = workspace.active_layout().expect("open");
        assert_eq!(layout.entries(), vec![number], "only the board is left");
    });
}

// ── what a pane is on ────────────────────────────────────────────

/// A member's number resolves to the entry it names, whatever kind it is.
#[gpui::test]
fn a_member_resolves_to_what_it_names(cx: &mut gpui::TestAppContext) {
    let scratch = Scratch::new("resolve");
    let workspace = opened(&scratch, cx);

    workspace.update(cx, |workspace, cx| {
        workspace.new_board(0, "Roadmap".into(), "ROAD", cx).ok();
        let board = workspace.projects[0].boards[0].number.expect("numbered");

        assert_eq!(
            workspace.showing_of(0, board),
            Some(Showing::Board(0)),
            "the board it names"
        );
        assert_eq!(
            workspace.showing_of(0, board + 900),
            None,
            "a number nothing answers to"
        );
    });
}

/// The number and the thing it names agree both ways round.
#[gpui::test]
fn a_number_and_what_it_names_agree(cx: &mut gpui::TestAppContext) {
    let scratch = Scratch::new("roundtrip");
    let workspace = opened(&scratch, cx);

    workspace.update(cx, |workspace, cx| {
        workspace.new_board(0, "Roadmap".into(), "ROAD", cx).ok();
        let number = workspace.projects[0].boards[0].number.expect("numbered");
        let showing = workspace.showing_of(0, number).expect("resolves");
        assert_eq!(workspace.number_of(0, showing), Some(number));
    });
}

/// Focusing a pane puts the project's selection on what that pane shows —
/// which is what makes every command without a pane of its own act on the
/// pane you are in.
#[gpui::test]
fn focusing_a_pane_moves_the_selection(cx: &mut gpui::TestAppContext) {
    let scratch = Scratch::new("select");
    let workspace = opened(&scratch, cx);

    workspace.update(cx, |workspace, cx| {
        workspace.new_board(0, "First".into(), "ONE", cx).ok();
        workspace.new_board(0, "Second".into(), "TWO", cx).ok();
        let second = workspace.projects[0].boards[0].number.expect("numbered");
        let first = workspace.projects[0].boards[1].number.expect("numbered");

        workspace.select_showing(0, Showing::Board(1), cx);
        assert_eq!(
            workspace.active_board().map(|board| board.number),
            Some(Some(first))
        );

        let showing = workspace.showing_of(0, second).expect("resolves");
        workspace.select_showing(0, showing, cx);
        assert_eq!(
            workspace.active_board().map(|board| board.number),
            Some(Some(second)),
            "the pane in front is what active_board answers for"
        );
    });
}

/// Two tables side by side each read their own rows: one page between them
/// would draw the same rows in both.
#[gpui::test]
fn two_table_panes_hold_two_pages(cx: &mut gpui::TestAppContext) {
    let scratch = Scratch::new("pages");
    let workspace = opened(&scratch, cx);

    workspace.update(cx, |workspace, cx| {
        // Tables are off by default — see [`Features::default`].
        workspace.settings.features.tables = true;
        // Through the model, which is what makes the store: opening a project
        // deliberately leaves it without one.
        workspace.new_table(cx).expect("a table");
        workspace.new_table(cx).expect("another");
        assert_eq!(workspace.projects[0].tables.len(), 2, "both made");

        let first = workspace.projects[0].tables[0].number.expect("numbered");
        let second = workspace.projects[0].tables[1].number.expect("numbered");
        workspace.arrange(0, first, second, Side::Right, cx);
        workspace.projects[0].table = Some(0);
        workspace.projects[0].reload_page();

        assert_eq!(
            workspace.projects[0].pages.len(),
            2,
            "a page each, not one between them"
        );
    });
}

/// Closing a pane drops that member and leaves the entry itself alone — the
/// cross on a pane leaves the arrangement, it does not delete anything.
#[gpui::test]
fn closing_a_pane_leaves_the_entry(cx: &mut gpui::TestAppContext) {
    let scratch = Scratch::new("close-pane");
    let workspace = opened(&scratch, cx);

    workspace.update(cx, |workspace, cx| {
        workspace.new_board(0, "Roadmap".into(), "ROAD", cx).ok();
        let board = workspace.projects[0].boards[0].number.expect("numbered");
        let boards = workspace.projects[0].boards.len();

        workspace.arrange(0, board, board + 500, Side::Right, cx);
        assert_eq!(workspace.active_layout().expect("open").leaves(), 2);

        workspace.close_pane(board + 500, cx);
        assert_eq!(
            workspace.active_layout().expect("open").entries(),
            vec![board]
        );
        assert_eq!(
            workspace.projects[0].boards.len(),
            boards,
            "the board it arranged is untouched"
        );
    });
}

/// Closing the last pane closes the layout with it: an arrangement of one
/// pane is not an arrangement, and its row would stand for something the
/// window is no longer doing.
#[gpui::test]
fn closing_the_last_pane_closes_the_layout(cx: &mut gpui::TestAppContext) {
    let scratch = Scratch::new("close-last");
    let workspace = opened(&scratch, cx);

    let id = workspace.update(cx, |workspace, cx| {
        workspace.new_board(0, "Roadmap".into(), "ROAD", cx).ok();
        let board = workspace.projects[0].boards[0].number.expect("numbered");
        workspace.arrange(0, board, board + 500, Side::Right, cx);
        let id = workspace.active_layout().expect("open").id.clone();

        workspace.close_pane(board + 500, cx);
        workspace.close_pane(board, cx);

        assert!(workspace.active_layout().is_none(), "the layout is gone");
        assert!(workspace.projects[0].layouts.is_empty());
        // What the last pane was on is still open, now as the whole window.
        assert_eq!(workspace.projects[0].board, Some(0));
        id
    });

    assert!(
        fs::Project::new(&scratch.0).layout(&id).is_none(),
        "the file goes too"
    );
}

/// Opening any other entry leaves the layout: it is one funnel, so no `open_*`
/// can forget to.
#[gpui::test]
fn opening_another_entry_leaves_the_layout(cx: &mut gpui::TestAppContext) {
    let scratch = Scratch::new("leave");
    let workspace = opened(&scratch, cx);

    workspace.update(cx, |workspace, cx| {
        workspace.new_board(0, "Roadmap".into(), "ROAD", cx).ok();
        workspace.arrange(0, 1, 2, Side::Right, cx);
        assert!(workspace.active_layout().is_some(), "arranged");

        workspace.open_board(0, 0, cx);
        assert!(
            workspace.active_layout().is_none(),
            "opening a board leaves the arrangement"
        );
        // The layout is left, not deleted — its row is still in the sidebar.
        assert_eq!(workspace.projects[0].layouts.len(), 1);
    });
}

/// And going back to it arranges the window again.
#[gpui::test]
fn a_layout_can_be_opened_again(cx: &mut gpui::TestAppContext) {
    let scratch = Scratch::new("return");
    let workspace = opened(&scratch, cx);

    workspace.update(cx, |workspace, cx| {
        workspace.new_board(0, "Roadmap".into(), "ROAD", cx).ok();
        workspace.arrange(0, 1, 2, Side::Right, cx);
        workspace.open_board(0, 0, cx);
        workspace.open_layout(0, 0, cx);

        assert!(workspace.active_layout().is_some());
        assert_eq!(workspace.active_layout().expect("open").leaves(), 2);
    });
}

// ── where a drop lands ───────────────────────────────────────────

/// The four edges tile the pane: wherever the pointer is, a release splits.
/// There is no middle that lights up and then does nothing.
#[test]
fn every_point_in_a_pane_names_an_edge() {
    use crate::view::root::Cydonia;
    for across in 0..=10 {
        for down in 0..=10 {
            let (x, y) = (across as f32 / 10., down as f32 / 10.);
            // Answers a side for every point — the type says so, and this is
            // the sweep that says no corner of it panics on the way.
            let _ = Cydonia::side_at(x, y);
        }
    }
}

/// Nearest edge wins, so each quarter of the pane points the way it lies.
#[test]
fn a_pointer_names_the_edge_it_is_nearest() {
    use crate::view::root::Cydonia;
    assert_eq!(Cydonia::side_at(0.1, 0.5), Side::Left);
    assert_eq!(Cydonia::side_at(0.9, 0.5), Side::Right);
    assert_eq!(Cydonia::side_at(0.5, 0.1), Side::Above);
    assert_eq!(Cydonia::side_at(0.5, 0.9), Side::Below);
}

/// The middle belongs to an edge too — it is not a dead spot. Dead centre is
/// a tie, and a tie goes to the first of them rather than to nothing.
#[test]
fn the_middle_belongs_to_an_edge() {
    use crate::view::root::Cydonia;
    assert_eq!(Cydonia::side_at(0.5, 0.5), Side::Left);
    assert_eq!(Cydonia::side_at(0.45, 0.5), Side::Left);
    assert_eq!(Cydonia::side_at(0.55, 0.5), Side::Right);
}

/// Each pane's bar names its own entry, not whichever one is in front.
/// Without this every bar in a layout would say the same thing.
#[gpui::test]
fn a_pane_bar_names_its_own_entry(cx: &mut gpui::TestAppContext) {
    use crate::view::root::Cydonia;
    let scratch = Scratch::new("bars");
    cx.update(|cx| bezel::theme::Theme::install(bezel::theme::Appearance::Light, cx));

    let window = cx.add_window(|window, cx| {
        let root = Cydonia::new(Settings::default(), state::State::default(), window, cx);
        root.workspace.update(cx, |workspace, cx| {
            workspace.open_project(scratch.0.clone(), cx);
            workspace.new_board(0, "First".into(), "ONE", cx).ok();
            workspace.new_board(0, "Second".into(), "TWO", cx).ok();
            // Newest first, so index 0 is Second and the window is on it.
            workspace.select_showing(0, Showing::Board(0), cx);
        });
        root
    });

    window
        .update(cx, |root, _, cx| {
            let front = root.toolbar_of(0, Showing::Board(0), cx).expect("in front");
            let beside = root.toolbar_of(0, Showing::Board(1), cx).expect("beside");
            assert_eq!(front.title, "Second");
            assert_eq!(
                beside.title, "First",
                "a pane names the entry it is on, not the focused one"
            );
        })
        .unwrap();
}

/// A pane is told its own width, not the window's. The transcript drops its
/// rail when the margins are too narrow to hold it, and measured against the
/// window a pane in a split would keep a rail there is no room for and draw
/// it over the prose.
#[gpui::test]
fn a_pane_is_told_its_own_width(cx: &mut gpui::TestAppContext) {
    use crate::view::root::Cydonia;
    let scratch = Scratch::new("width");
    cx.update(|cx| bezel::theme::Theme::install(bezel::theme::Appearance::Light, cx));

    let window = cx.add_window(|window, cx| {
        let root = Cydonia::new(Settings::default(), state::State::default(), window, cx);
        root.workspace.update(cx, |workspace, cx| {
            workspace.open_project(scratch.0.clone(), cx);
            workspace.arrange(0, 1, 2, Side::Right, cx);
        });
        root
    });

    window
        .update(cx, |root, _, cx| {
            assert_eq!(root.width_share(Some(1), cx), 0.5, "half the column");
            assert_eq!(root.width_share(Some(2), cx), 0.5);
            // Nothing a pane is on, and the single-pane case, are the whole of
            // it rather than nothing at all.
            assert_eq!(root.width_share(Some(99), cx), 1.);
            assert_eq!(root.width_share(None, cx), 1.);

            // Zoomed, the pane in front has the window to itself.
            root.workspace
                .update(cx, |workspace, cx| workspace.zoom_pane(2, cx));
            assert_eq!(root.width_share(Some(2), cx), 1., "zoomed");
        })
        .unwrap();
}

/// Stacking panes leaves each of them the full width: only a split across
/// narrows one.
#[gpui::test]
fn stacked_panes_keep_the_width(cx: &mut gpui::TestAppContext) {
    use crate::view::root::Cydonia;
    let scratch = Scratch::new("stacked");
    cx.update(|cx| bezel::theme::Theme::install(bezel::theme::Appearance::Light, cx));

    let window = cx.add_window(|window, cx| {
        let root = Cydonia::new(Settings::default(), state::State::default(), window, cx);
        root.workspace.update(cx, |workspace, cx| {
            workspace.open_project(scratch.0.clone(), cx);
            workspace.arrange(0, 1, 2, Side::Below, cx);
        });
        root
    });

    window
        .update(cx, |root, _, cx| {
            assert_eq!(root.width_share(Some(1), cx), 1.);
            assert_eq!(root.width_share(Some(2), cx), 1.);
        })
        .unwrap();
}
