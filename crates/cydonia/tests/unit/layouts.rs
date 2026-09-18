//! Layouts are born from a drag, not from a menu: the first entry dropped on
//! the one already open mints one, and every drag after it lands in the same
//! layout.
//!
//! A layout is the window's, not a project's — it can hold a pane on one
//! repository beside a pane on another — so these open scratch projects and
//! point the config directory somewhere of their own.

use super::*;
use crate::model::layouts as store;
use crate::model::workspace::Showing;
use crate::model::{settings::Settings, state};
use artifact::layout::{Member, Side};
use gpui::AppContext as _;

/// A scratch project or two, torn down when the test ends — and a config
/// directory beside them that the test's writes land in.
///
/// [`Workspace::save`] writes `state.toml` under [`crate::model::settings::dir`]
/// for real, and layouts are kept beside it. Without somewhere else to put them
/// a test rewrites the project list of whoever ran it, with scratch paths that
/// are about to be deleted — and the app they open next lists nothing.
struct Scratch(std::path::PathBuf);

impl Scratch {
    fn new(name: &str) -> Self {
        let root =
            std::env::temp_dir().join(format!("cydonia-layouts-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("config")).unwrap();
        // Before any workspace exists, so nothing has saved yet. Sound here
        // because nextest gives each test its own process and this runs before
        // the app spawns a thread.
        unsafe { std::env::set_var("XDG_CONFIG_HOME", root.join("config")) };
        Self(root)
    }

    /// A project directory under it — the thing a project *is*.
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

/// A workspace on one scratch project holding two boards, and the members that
/// name them.
fn two_boards(
    scratch: &Scratch,
    cx: &mut gpui::TestAppContext,
) -> (gpui::Entity<Workspace>, Member, Member) {
    cx.update(|cx| bezel::theme::Theme::install(bezel::theme::Appearance::Light, cx));
    let workspace = cx.new(|cx| Workspace::new(Settings::default(), state::State::default(), cx));
    let (a, b) = workspace.update(cx, |workspace, cx| {
        workspace.open_project(scratch.project("one"), cx);
        workspace.new_board(0, "First".into(), "ONE", cx).ok();
        workspace.new_board(0, "Second".into(), "TWO", cx).ok();
        (
            workspace.member_of(0, Showing::Board(0)).expect("a member"),
            workspace.member_of(0, Showing::Board(1)).expect("a member"),
        )
    });
    (workspace, a, b)
}

/// The first drag mints the layout over the pane that was already there, and
/// names it `layout-1`.
#[gpui::test]
fn the_first_drag_mints_a_layout(cx: &mut gpui::TestAppContext) {
    let scratch = Scratch::new("first");
    let (workspace, a, b) = two_boards(&scratch, cx);

    workspace.update(cx, |workspace, cx| {
        assert!(workspace.layouts.is_empty(), "none to start");

        assert_eq!(workspace.arrange(&a, &b, Side::Right, cx), Some(0));
        let layout = workspace.active_layout().expect("open");
        assert_eq!(layout.name, "layout-1");
        assert_eq!(layout.entries(), vec![a, b], "the pane it was made over");
    });
}

/// Every drag after the first lands in the same layout rather than minting
/// another.
#[gpui::test]
fn later_drags_land_in_the_same_layout(cx: &mut gpui::TestAppContext) {
    let scratch = Scratch::new("same");
    let (workspace, a, b) = two_boards(&scratch, cx);

    workspace.update(cx, |workspace, cx| {
        // Made before the first drag: making an entry opens it, and opening
        // one leaves the arrangement — so a board made half way through would
        // have the next drag mint a second layout.
        workspace.new_board(0, "Third".into(), "THR", cx).ok();
        let c = workspace.member_of(0, Showing::Board(0)).expect("a member");

        workspace.arrange(&a, &b, Side::Right, cx);
        workspace.arrange(&b, &c, Side::Below, cx);

        assert_eq!(workspace.layouts.len(), 1, "one layout");
        assert_eq!(workspace.active_layout().expect("open").leaves(), 3);
    });
}

/// A layout is kept beside the config, not in any project: it can hold panes
/// from several, so it belongs to none of them.
#[gpui::test]
fn a_layout_is_written_beside_the_config(cx: &mut gpui::TestAppContext) {
    let scratch = Scratch::new("written");
    let (workspace, a, b) = two_boards(&scratch, cx);

    let id = workspace.update(cx, |workspace, cx| {
        workspace.arrange(&a, &b, Side::Right, cx);
        workspace.active_layout().expect("open").id.clone()
    });

    let read = store::read(&id).expect("on disk");
    assert_eq!(read.entries().len(), 2);
    assert!(
        !scratch.0.join("one/.cydonia/layouts").exists(),
        "and nothing is written into the project"
    );
}

/// The whole point of a layout being the window's: one arrangement can hold a
/// pane on one repository beside a pane on another.
#[gpui::test]
fn a_layout_can_span_projects(cx: &mut gpui::TestAppContext) {
    let scratch = Scratch::new("across");
    cx.update(|cx| bezel::theme::Theme::install(bezel::theme::Appearance::Light, cx));
    let workspace = cx.new(|cx| Workspace::new(Settings::default(), state::State::default(), cx));

    let id = workspace.update(cx, |workspace, cx| {
        workspace.open_project(scratch.project("one"), cx);
        workspace.open_project(scratch.project("two"), cx);
        workspace.new_board(0, "Here".into(), "ONE", cx).ok();
        workspace.new_board(1, "There".into(), "TWO", cx).ok();
        let here = workspace.member_of(0, Showing::Board(0)).expect("a member");
        let there = workspace.member_of(1, Showing::Board(0)).expect("a member");
        assert_ne!(here.project, there.project, "two repositories");

        workspace.arrange(&here, &there, Side::Right, cx);
        let layout = workspace.active_layout().expect("open");
        assert_eq!(layout.leaves(), 2);
        // Both resolve, each to the project it is in.
        assert_eq!(workspace.showing_of(&here).map(|(at, _)| at), Some(0));
        assert_eq!(workspace.showing_of(&there).map(|(at, _)| at), Some(1));
        layout.id.clone()
    });

    let read = store::read(&id).expect("on disk");
    let projects: Vec<_> = read
        .entries()
        .into_iter()
        .map(|member| member.project)
        .collect();
    assert_ne!(projects[0], projects[1], "written as two projects' entries");
}

/// A member whose project is not open resolves to nothing — the pane draws as
/// an empty one rather than as somebody else's entry.
#[gpui::test]
fn a_member_in_a_shut_project_resolves_to_nothing(cx: &mut gpui::TestAppContext) {
    let scratch = Scratch::new("shut");
    let (workspace, a, _) = two_boards(&scratch, cx);

    workspace.update(cx, |workspace, cx| {
        let elsewhere = Member::new(
            scratch.project("two"),
            artifact::layout::Kind::Board,
            a.id.clone(),
        );
        assert!(workspace.showing_of(&elsewhere).is_none());
        // The same id in a project that *is* open still resolves.
        assert!(workspace.showing_of(&a).is_some());
        let _ = cx;
    });
}

/// Opening any entry leaves the arrangement — one funnel, so no `open_*` can
/// forget to. The layout is left, not deleted.
#[gpui::test]
fn opening_another_entry_leaves_the_layout(cx: &mut gpui::TestAppContext) {
    let scratch = Scratch::new("leave");
    let (workspace, a, b) = two_boards(&scratch, cx);

    workspace.update(cx, |workspace, cx| {
        workspace.arrange(&a, &b, Side::Right, cx);
        assert!(workspace.active_layout().is_some(), "arranged");

        workspace.open_board(0, 0, cx);
        assert!(workspace.active_layout().is_none(), "left it");
        assert_eq!(workspace.layouts.len(), 1, "but it is still there");

        workspace.open_layout(0, cx);
        assert!(workspace.active_layout().is_some(), "and can be reopened");
    });
}

/// An entry is in one layout at a time, the way a pane is in one tmux window.
#[gpui::test]
fn an_entry_belongs_to_one_layout(cx: &mut gpui::TestAppContext) {
    let scratch = Scratch::new("exclusive");
    let (workspace, a, b) = two_boards(&scratch, cx);

    workspace.update(cx, |workspace, cx| {
        workspace.new_board(0, "Third".into(), "THR", cx).ok();
        let c = workspace.member_of(0, Showing::Board(0)).expect("a member");

        workspace.arrange(&a, &c, Side::Right, cx);
        let first = workspace.active_layout().expect("open").id.clone();
        workspace.leave_layout();
        workspace.arrange(&b, &c, Side::Right, cx);

        let held = workspace.layout_holding(&c).expect("in a layout");
        assert_ne!(workspace.layouts[held].id, first, "it moved");
        let left = workspace
            .layouts
            .iter()
            .find(|layout| layout.id == first)
            .expect("still there");
        assert!(!left.contains(&c), "and left the one it was in");
    });
}

/// Closing a pane down to one closes the layout with it — an arrangement of
/// one pane is not an arrangement — and the window is left on the pane that
/// would have been alone in it.
#[gpui::test]
fn closing_down_to_one_pane_closes_the_layout(cx: &mut gpui::TestAppContext) {
    let scratch = Scratch::new("close-last");
    let (workspace, a, b) = two_boards(&scratch, cx);

    let id = workspace.update(cx, |workspace, cx| {
        workspace.arrange(&a, &b, Side::Right, cx);
        let id = workspace.active_layout().expect("open").id.clone();

        workspace.close_pane(&b, cx);

        assert!(workspace.active_layout().is_none(), "the layout is gone");
        assert!(workspace.layouts.is_empty());
        assert_eq!(
            workspace.member_of(0, Showing::Board(0)).as_ref(),
            Some(&a),
            "and the window is on the pane that was left",
        );
        assert_eq!(workspace.projects[0].board, Some(0));
        id
    });
    assert!(store::read(&id).is_none(), "the file goes too");
}

/// Moving a pane exchanges it with the one across the seam and is written back.
#[gpui::test]
fn moving_a_pane_swaps_it_and_is_written(cx: &mut gpui::TestAppContext) {
    let scratch = Scratch::new("move-pane");
    let (workspace, a, b) = two_boards(&scratch, cx);

    let id = workspace.update(cx, |workspace, cx| {
        workspace.arrange(&a, &b, Side::Right, cx);
        assert_eq!(workspace.neighbour_pane(&a, Side::Left), None, "the edge");
        assert!(!workspace.move_pane(&a, Side::Left, cx), "nowhere to go");

        assert!(workspace.move_pane(&a, Side::Right, cx));
        assert_eq!(
            workspace.active_layout().expect("open").entries(),
            vec![b.clone(), a.clone()],
            "they changed places"
        );
        workspace.active_layout().expect("open").id.clone()
    });

    assert_eq!(store::read(&id).expect("on disk").entries(), vec![b, a]);
}

/// Putting a layout away keeps the arrangement and stops it holding its
/// members — the sidebar lists them where they would be without it.
#[gpui::test]
fn archiving_a_layout_releases_its_members(cx: &mut gpui::TestAppContext) {
    let scratch = Scratch::new("archive");
    let (workspace, a, b) = two_boards(&scratch, cx);

    let id = workspace.update(cx, |workspace, cx| {
        workspace.arrange(&a, &b, Side::Right, cx);
        let id = workspace.active_layout().expect("open").id.clone();
        assert_eq!(workspace.layout_holding(&a), Some(0));

        workspace.archive_layout(0, true, cx);
        assert_eq!(workspace.layout_holding(&a), None, "released");
        assert!(workspace.active_layout().is_none(), "and not on screen");
        id
    });

    let read = store::read(&id).expect("on disk");
    assert!(read.archived);
    assert_eq!(read.entries().len(), 2, "still arranged as it was");
}

/// A renamed layout keeps its name across a re-read.
#[gpui::test]
fn a_rename_is_written(cx: &mut gpui::TestAppContext) {
    let scratch = Scratch::new("rename");
    let (workspace, a, b) = two_boards(&scratch, cx);

    let id = workspace.update(cx, |workspace, cx| {
        workspace.arrange(&a, &b, Side::Right, cx);
        let id = workspace.active_layout().expect("open").id.clone();
        workspace.rename_layout(&id, "Auth work".into(), cx);
        id
    });

    assert_eq!(store::read(&id).expect("on disk").name, "Auth work");
}

/// A member whose entry has gone leaves the layout, so no pane is left on a
/// thing that is not there.
#[gpui::test]
fn a_member_that_has_gone_is_pruned(cx: &mut gpui::TestAppContext) {
    let scratch = Scratch::new("prune");
    let (workspace, a, b) = two_boards(&scratch, cx);

    workspace.update(cx, |workspace, cx| {
        workspace.arrange(&a, &b, Side::Right, cx);
        // Boards are listed newest first, so index 1 is the older of the two —
        // which is `b`, the one `two_boards` took from `Board(1)`.
        workspace.delete_board(0, 1, cx);
        workspace.prune_layouts(cx);

        let layout = workspace.active_layout().expect("open");
        assert_eq!(layout.entries(), vec![a], "only the one still there");
    });
}

/// An entry going away from the list takes its pane with it, whether or not
/// the layout holding it is the one in front.
#[gpui::test]
fn an_entry_can_be_dropped_from_the_layout_holding_it(cx: &mut gpui::TestAppContext) {
    let scratch = Scratch::new("dropped");
    let (workspace, a, b) = two_boards(&scratch, cx);

    workspace.update(cx, |workspace, cx| {
        workspace.new_board(0, "Third".into(), "THR", cx).ok();
        let c = workspace.member_of(0, Showing::Board(0)).expect("a member");
        workspace.arrange(&a, &b, Side::Right, cx);
        workspace.arrange(&b, &c, Side::Below, cx);
        assert_eq!(workspace.active_layout().expect("open").leaves(), 3);

        workspace.drop_from_layouts(&b, cx);

        let layout = workspace.active_layout().expect("still open");
        assert_eq!(layout.leaves(), 2);
        assert!(!layout.contains(&b), "the pane went with the entry");
        assert_eq!(workspace.layout_holding(&b), None);
    });
}

/// Taking a pane out down to one takes the layout with it — the same rule
/// closing one by hand follows.
#[gpui::test]
fn dropping_a_pane_down_to_one_drops_the_layout(cx: &mut gpui::TestAppContext) {
    let scratch = Scratch::new("last");
    let (workspace, a, b) = two_boards(&scratch, cx);

    workspace.update(cx, |workspace, cx| {
        workspace.arrange(&a, &b, Side::Right, cx);
        workspace.drop_from_layouts(&a, cx);

        assert!(workspace.layouts.is_empty(), "nothing left to arrange");
        assert_eq!(workspace.layout_holding(&b), None, "and b is loose again");
    });
}

/// Sending into a session that is a pane of the open layout leaves the window
/// where it was. Sending is not opening — the arrangement is the working
/// context, and a message must not throw it away.
#[gpui::test]
fn sending_into_a_pane_stays_in_the_layout(cx: &mut gpui::TestAppContext) {
    let scratch = Scratch::new("send-in-layout");
    cx.update(|cx| bezel::theme::Theme::install(bezel::theme::Appearance::Light, cx));
    // `send` refuses and says so while sessions are off, and would never reach
    // the bookkeeping under test.
    let mut settings = Settings::default();
    settings.features.sessions = true;
    let workspace = cx.new(|cx| Workspace::new(settings, state::State::default(), cx));

    workspace.update(cx, |workspace, cx| {
        workspace.open_project(scratch.project("one"), cx);
        workspace.new_board(0, "First".into(), "ONE", cx).ok();
        let board = workspace.member_of(0, Showing::Board(0)).expect("a member");

        // Restored rather than opened: a session made through `new_session`
        // starts an agent, and what is under test is the bookkeeping `send`
        // does around one.
        let path = workspace.projects[0].path.clone();
        workspace.projects[0]
            .sessions
            .push(resting_session(7, &path));
        workspace.projects[0].active = Some(7);
        let chat = workspace
            .member_of(0, Showing::Session(7))
            .expect("a member");

        workspace.arrange(&board, &chat, Side::Right, cx);
        assert!(workspace.active_layout().is_some(), "arranged to start");

        workspace.send(7, "ship it".into(), cx);

        assert!(
            workspace.active_layout().is_some(),
            "and still arranged after",
        );
    });
}

/// A session with a record and no agent behind it — and none it could start,
/// since `send` resumes a session whose entry names a command and that would
/// put a process behind a unit test.
fn resting_session(id: u64, project: &std::path::Path) -> crate::model::session::ChatSession {
    let record = serde_json::from_value(serde_json::json!({
        "id": "test", "agent": "test", "title": "", "name": null,
        "updated": 1, "items": []
    }))
    .expect("a record");
    crate::model::session::ChatSession::restore(
        id,
        project.to_path_buf(),
        crate::model::settings::Agent {
            name: "test".into(),
            id: None,
            command: String::new(),
            args: Vec::new(),
            env: Default::default(),
        },
        record,
    )
}

/// An entry dropped on a pane's bar joins that pane rather than splitting it,
/// and the arrangement stays the shape it was.
#[gpui::test]
fn stacking_joins_a_pane_instead_of_splitting_it(cx: &mut gpui::TestAppContext) {
    let scratch = Scratch::new("stack");
    cx.update(|cx| bezel::theme::Theme::install(bezel::theme::Appearance::Light, cx));
    let workspace = cx.new(|cx| Workspace::new(Settings::default(), state::State::default(), cx));

    workspace.update(cx, |workspace, cx| {
        workspace.open_project(scratch.project("one"), cx);
        for (name, key) in [("First", "ONE"), ("Second", "TWO"), ("Third", "THR")] {
            workspace.new_board(0, name.into(), key, cx).ok();
        }
        let a = workspace.member_of(0, Showing::Board(0)).expect("a member");
        let b = workspace.member_of(0, Showing::Board(1)).expect("a member");
        let c = workspace.member_of(0, Showing::Board(2)).expect("a member");
        workspace.arrange(&a, &b, Side::Right, cx);
        assert_eq!(workspace.active_layout().expect("open").leaves(), 2);

        workspace.stack_pane(&a, &c, cx);

        let layout = workspace.active_layout().expect("open");
        assert_eq!(layout.leaves(), 2, "still two panes");
        assert_eq!(layout.panes(), vec![a.clone(), b.clone()]);
        assert_eq!(workspace.stack_of(&a), vec![a.clone(), c.clone()]);
        // Named from the tab as well as from the pane: either says which pane.
        assert_eq!(workspace.stack_of(&c), vec![a, c]);
        assert_eq!(workspace.stack_of(&b), vec![b]);
    });
}

/// Closing a tab of a pane holding several leaves the pane — and so the
/// layout — exactly where it was.
#[gpui::test]
fn closing_a_tab_leaves_the_pane_and_the_layout_alone(cx: &mut gpui::TestAppContext) {
    let scratch = Scratch::new("close-tab");
    cx.update(|cx| bezel::theme::Theme::install(bezel::theme::Appearance::Light, cx));
    let workspace = cx.new(|cx| Workspace::new(Settings::default(), state::State::default(), cx));

    workspace.update(cx, |workspace, cx| {
        workspace.open_project(scratch.project("one"), cx);
        for (name, key) in [("First", "ONE"), ("Second", "TWO"), ("Third", "THR")] {
            workspace.new_board(0, name.into(), key, cx).ok();
        }
        let a = workspace.member_of(0, Showing::Board(0)).expect("a member");
        let b = workspace.member_of(0, Showing::Board(1)).expect("a member");
        let c = workspace.member_of(0, Showing::Board(2)).expect("a member");
        workspace.arrange(&a, &b, Side::Right, cx);
        workspace.stack_pane(&a, &c, cx);

        // Two panes and one of them holding a tab: closing that tab must not
        // read as closing down to one pane, which drops the layout.
        workspace.close_pane(&c, cx);

        let layout = workspace.active_layout().expect("still open");
        assert_eq!(layout.leaves(), 2);
        assert_eq!(workspace.stack_of(&a), vec![a.clone()]);

        // Now it is the pane's last, so the pane goes — and with it the
        // layout, which is down to one.
        workspace.close_pane(&a, cx);
        assert!(workspace.active_layout().is_none(), "the layout is gone");
        assert_eq!(workspace.projects[0].board, Some(1), "left on what remains");
    });
}

/// An entry is in one pane at a time: stacking one already arranged elsewhere
/// moves it rather than showing it twice.
#[gpui::test]
fn stacking_an_arranged_entry_moves_it(cx: &mut gpui::TestAppContext) {
    let scratch = Scratch::new("stack-move");
    cx.update(|cx| bezel::theme::Theme::install(bezel::theme::Appearance::Light, cx));
    let workspace = cx.new(|cx| Workspace::new(Settings::default(), state::State::default(), cx));

    workspace.update(cx, |workspace, cx| {
        workspace.open_project(scratch.project("one"), cx);
        for (name, key) in [("First", "ONE"), ("Second", "TWO"), ("Third", "THR")] {
            workspace.new_board(0, name.into(), key, cx).ok();
        }
        let a = workspace.member_of(0, Showing::Board(0)).expect("a member");
        let b = workspace.member_of(0, Showing::Board(1)).expect("a member");
        let c = workspace.member_of(0, Showing::Board(2)).expect("a member");
        workspace.arrange(&a, &b, Side::Right, cx);
        workspace.arrange(&b, &c, Side::Below, cx);
        assert_eq!(workspace.active_layout().expect("open").leaves(), 3);

        workspace.stack_pane(&a, &c, cx);

        let layout = workspace.active_layout().expect("open");
        assert_eq!(layout.leaves(), 2, "the pane it left collapsed");
        assert_eq!(layout.entries().len(), 3, "and nothing is shown twice");
        assert_eq!(workspace.stack_of(&a), vec![a, c]);
    });
}
