//! Spaces are born from a drag, not from a menu: the first entry dropped on
//! the one already open mints one, and every drag after it lands in the same
//! space.
//!
//! A space is the window's, not a project's — it can hold a pane on one
//! repository beside a pane on another — so these open scratch projects and
//! point the config directory somewhere of their own.

use super::*;
use crate::model::{
    settings::Settings,
    spaces::Shelf,
    state,
    workspace::{Member, Showing},
};
use artifact::{kind::Kind, space::Side};

/// A scratch project or two, torn down when the test ends — and a config
/// directory beside them that the test's writes land in.
///
/// [`Workspace::save`] writes `state.toml` under [`crate::model::settings::dir`]
/// for real, and spaces are kept beside it. Without somewhere else to put them
/// a test rewrites the project list of whoever ran it, with scratch paths that
/// are about to be deleted — and the app they open next lists nothing.
struct Scratch(std::path::PathBuf);

impl Scratch {
    fn new(name: &str) -> Self {
        let root =
            std::env::temp_dir().join(format!("cydonia-spaces-{name}-{}", std::process::id()));
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
            workspace.board_member(0, 0).expect("a member"),
            workspace.board_member(0, 1).expect("a member"),
        )
    });
    (workspace, a, b)
}

/// The first drag mints the space over the pane that was already there, and
/// names it `space-1`.
#[gpui::test]
fn the_first_drag_mints_a_space(cx: &mut gpui::TestAppContext) {
    let scratch = Scratch::new("first");
    let (workspace, a, b) = two_boards(&scratch, cx);

    workspace.update(cx, |workspace, cx| {
        assert!(workspace.spaces.is_empty(), "none to start");

        assert_eq!(workspace.arrange(&a, &b, Side::Right, cx), Some(0));
        let space = workspace.active_space().expect("open");
        assert_eq!(space.name, "space-1");
        assert_eq!(space.entries(), vec![a, b], "the pane it was made over");
    });
}

/// Every drag after the first lands in the same space rather than minting
/// another.
#[gpui::test]
fn later_drags_land_in_the_same_space(cx: &mut gpui::TestAppContext) {
    let scratch = Scratch::new("same");
    let (workspace, a, b) = two_boards(&scratch, cx);

    workspace.update(cx, |workspace, cx| {
        // Made before the first drag: making an entry opens it, and opening
        // one leaves the arrangement — so a board made half way through would
        // have the next drag mint a second space.
        workspace.new_board(0, "Third".into(), "THR", cx).ok();
        let c = workspace.board_member(0, 0).expect("a member");

        workspace.arrange(&a, &b, Side::Right, cx);
        workspace.arrange(&b, &c, Side::Below, cx);

        assert_eq!(workspace.spaces.len(), 1, "one space");
        assert_eq!(workspace.active_space().expect("open").leaves(), 3);
    });
}

/// A space is kept beside the config, not in any project: it can hold panes
/// from several, so it belongs to none of them.
#[gpui::test]
fn a_space_is_written_beside_the_config(cx: &mut gpui::TestAppContext) {
    let scratch = Scratch::new("written");
    let (workspace, a, b) = two_boards(&scratch, cx);

    let id = workspace.update(cx, |workspace, cx| {
        workspace.arrange(&a, &b, Side::Right, cx);
        workspace.active_space().expect("open").id.clone()
    });

    let read = Shelf::read(&id).expect("on disk");
    assert_eq!(read.entries().len(), 2);
    assert!(
        !scratch.0.join("one/.cydonia/spaces").exists(),
        "and nothing is written into the project"
    );
}

/// A project opened under one path has the id of the path it resolves to —
/// the scratch directory sits under `/var`, which macOS resolves to
/// `/private/var`.
#[gpui::test]
fn a_project_is_found_by_the_path_it_resolves_to(cx: &mut gpui::TestAppContext) {
    let scratch = Scratch::new("settled");
    let (workspace, _, _) = two_boards(&scratch, cx);
    let opened = scratch.project("one");
    let settled = opened.canonicalize().unwrap();

    workspace.update(cx, |workspace, _| {
        let id = workspace.projects[0].id;
        assert_eq!(workspace.intern(&opened), id);
        assert_eq!(workspace.intern(&settled), id);
        let other = workspace.intern(&scratch.0.join("other"));
        assert_ne!(other, id);
        assert_eq!(workspace.project_ix(other), None);
    });
}

/// The whole point of a space being the window's: one arrangement can hold a
/// pane on one repository beside a pane on another.
#[gpui::test]
fn a_space_can_span_projects(cx: &mut gpui::TestAppContext) {
    let scratch = Scratch::new("across");
    cx.update(|cx| bezel::theme::Theme::install(bezel::theme::Appearance::Light, cx));
    let workspace = cx.new(|cx| Workspace::new(Settings::default(), state::State::default(), cx));

    let id = workspace.update(cx, |workspace, cx| {
        workspace.open_project(scratch.project("one"), cx);
        workspace.open_project(scratch.project("two"), cx);
        workspace.new_board(0, "Here".into(), "ONE", cx).ok();
        workspace.new_board(1, "There".into(), "TWO", cx).ok();
        let here = workspace.board_member(0, 0).expect("a member");
        let there = workspace.board_member(1, 0).expect("a member");
        assert_ne!(here.project(), there.project(), "two repositories");

        workspace.arrange(&here, &there, Side::Right, cx);
        let space = workspace.active_space().expect("open");
        assert_eq!(space.leaves(), 2);
        // Both resolve, each to the project it is in.
        assert_eq!(workspace.showing_of(&here, cx).map(|(at, _)| at), Some(0));
        assert_eq!(workspace.showing_of(&there, cx).map(|(at, _)| at), Some(1));
        space.id.clone()
    });

    let read = Shelf::read(&id).expect("on disk");
    let projects: Vec<_> = read
        .entries()
        .into_iter()
        .map(|member| member.project().cloned())
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
            workspace.intern(&scratch.project("two")),
            Kind::Board,
            a.id(),
        );
        assert!(workspace.showing_of(&elsewhere, cx).is_none());
        // The same id in a project that *is* open still resolves.
        assert!(workspace.showing_of(&a, cx).is_some());
        let _ = cx;
    });
}

/// An entry opened as a tab of its own leaves the space behind its tab. The
/// space is left, not deleted.
#[gpui::test]
fn opening_another_entry_leaves_the_space(cx: &mut gpui::TestAppContext) {
    let scratch = Scratch::new("leave");
    let (workspace, a, b) = two_boards(&scratch, cx);

    workspace.update(cx, |workspace, cx| {
        workspace.arrange(&a, &b, Side::Right, cx);
        assert!(workspace.active_space().is_some(), "arranged");

        workspace.open_tab(a.clone(), crate::model::workspace::Opening::Kept, cx);
        assert!(workspace.active_space().is_none(), "left it");
        assert_eq!(workspace.spaces.len(), 1, "but it is still there");

        workspace.open_space(0, cx);
        assert!(workspace.active_space().is_some(), "and can be reopened");
    });
}

/// An entry is in one space at a time, the way a pane is in one tmux window.
/// Dragged into a second one it moves, and the one it came out of is left with
/// a single pane — which is no arrangement, so it goes.
#[gpui::test]
fn an_entry_belongs_to_one_space(cx: &mut gpui::TestAppContext) {
    let scratch = Scratch::new("exclusive");
    let (workspace, a, b) = two_boards(&scratch, cx);

    workspace.update(cx, |workspace, cx| {
        workspace.new_board(0, "Third".into(), "THR", cx).ok();
        let c = workspace.board_member(0, 2).expect("a member");

        workspace.arrange(&a, &c, Side::Right, cx);
        let first = workspace.active_space().expect("open").id.clone();
        workspace.open_tab(b.clone(), crate::model::workspace::Opening::Kept, cx);
        workspace.arrange(&b, &c, Side::Right, cx);

        let held = workspace.space_holding(&c).expect("in a space");
        assert_ne!(workspace.spaces[held].id, first, "it moved");
        assert!(
            !workspace.spaces.iter().any(|space| space.id == first),
            "and the one it left had nothing else to arrange"
        );
    });
}

/// Closing a pane down to one closes the space with it — an arrangement of
/// one pane is not an arrangement — and the window is left on the pane that
/// would have been alone in it.
#[gpui::test]
fn closing_down_to_one_pane_closes_the_space(cx: &mut gpui::TestAppContext) {
    let scratch = Scratch::new("close-last");
    let (workspace, a, b) = two_boards(&scratch, cx);

    let id = workspace.update(cx, |workspace, cx| {
        workspace.arrange(&a, &b, Side::Right, cx);
        let id = workspace.active_space().expect("open").id.clone();

        workspace.close_pane(&b, cx);

        assert!(workspace.active_space().is_none(), "the space is gone");
        assert!(workspace.spaces.is_empty());
        assert_eq!(
            workspace.board_member(0, 0).as_ref(),
            Some(&a),
            "and the window is on the pane that was left",
        );
        assert_eq!(workspace.projects[0].board, Some(0));
        id
    });
    assert!(Shelf::read(&id).is_none(), "the file goes too");
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
            workspace.active_space().expect("open").entries(),
            vec![b.clone(), a.clone()],
            "they changed places"
        );
        workspace.active_space().expect("open").id.clone()
    });

    let written = |member: Member| member.map(|_| scratch.project("one"));
    assert_eq!(
        Shelf::read(&id).expect("on disk").entries(),
        vec![written(b), written(a)]
    );
}

/// Archiving a space is archiving what it arranges, and the arrangement goes
/// with them — there is no put-away space to come back to.
///
/// The walk that archives the members takes each one out of the space as it
/// goes, which is why the drop is by id: an index read before the walk names
/// some other space by the end of it.
#[gpui::test]
fn a_space_is_dropped_by_the_id_it_was_named_with(cx: &mut gpui::TestAppContext) {
    let scratch = Scratch::new("archive");
    let (workspace, a, b) = two_boards(&scratch, cx);

    let id = workspace.update(cx, |workspace, cx| {
        workspace.arrange(&a, &b, Side::Right, cx);
        let id = workspace.active_space().expect("open").id.clone();
        assert_eq!(workspace.space_holding(&a), Some(0));

        // What archiving a member does, and what leaves the index stale: the
        // pane it took out was one of two, so the arrangement went with it.
        workspace.drop_from_spaces(&a, cx);
        assert!(workspace.spaces.is_empty(), "an arrangement of one is none");

        // And the drop that follows names one that is already gone, which is
        // the case an index would have got wrong.
        workspace.delete_space_id(&id, cx);
        assert!(workspace.active_space().is_none(), "and not on screen");
        id
    });

    assert!(Shelf::read(&id).is_none(), "the file goes too");
}

/// An entry is in one space at a time, and that holds for the pane dropped
/// *on* as much as for the one arriving.
///
/// A space minted over a target another space already holds would put that
/// entry in two at once — which the sidebar then lists twice, once under each.
#[gpui::test]
fn the_pane_dropped_on_leaves_the_space_that_held_it(cx: &mut gpui::TestAppContext) {
    let scratch = Scratch::new("exclusive");
    let (workspace, a, b) = two_boards(&scratch, cx);
    let c = workspace.update(cx, |workspace, cx| {
        workspace.new_board(0, "Third".into(), "THR", cx).ok();
        workspace.board_member(0, 2).expect("a member")
    });

    workspace.update(cx, |workspace, cx| {
        workspace.arrange(&a, &b, Side::Right, cx);
        let first = workspace.active_space().expect("open").id.clone();

        // Out on its own and dragged into a second arrangement, which is what
        // opening an entry the space does not hold and then dropping beside it
        // does.
        workspace.open_tab(a.clone(), crate::model::workspace::Opening::Kept, cx);
        workspace.arrange(&a, &c, Side::Right, cx);

        let open = workspace.active_space().expect("open");
        assert_ne!(open.id, first, "a new one, over the pane that was there");
        assert_eq!(open.entries(), vec![a.clone(), c]);
        assert_eq!(
            workspace.spaces.len(),
            1,
            "the one it left held two panes, so it was not left holding one"
        );
        assert_eq!(workspace.space_holding(&a), Some(0), "and in just the one");
    });
}

/// A renamed space keeps its name across a re-read.
#[gpui::test]
fn a_rename_is_written(cx: &mut gpui::TestAppContext) {
    let scratch = Scratch::new("rename");
    let (workspace, a, b) = two_boards(&scratch, cx);

    let id = workspace.update(cx, |workspace, cx| {
        workspace.arrange(&a, &b, Side::Right, cx);
        let id = workspace.active_space().expect("open").id.clone();
        workspace.rename_space(&id, "Auth work".into(), cx);
        id
    });

    assert_eq!(Shelf::read(&id).expect("on disk").name, "Auth work");
}

/// A member whose entry has gone leaves the space, so no pane is left on a
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
        workspace.prune_spaces(cx);

        let space = workspace.active_space().expect("open");
        assert_eq!(space.entries(), vec![a], "only the one still there");
    });
}

/// An entry going away from the list takes its pane with it, whether or not
/// the space holding it is the one in front.
#[gpui::test]
fn an_entry_can_be_dropped_from_the_space_holding_it(cx: &mut gpui::TestAppContext) {
    let scratch = Scratch::new("dropped");
    let (workspace, a, b) = two_boards(&scratch, cx);

    workspace.update(cx, |workspace, cx| {
        workspace.new_board(0, "Third".into(), "THR", cx).ok();
        let c = workspace.board_member(0, 0).expect("a member");
        workspace.arrange(&a, &b, Side::Right, cx);
        workspace.arrange(&b, &c, Side::Below, cx);
        assert_eq!(workspace.active_space().expect("open").leaves(), 3);

        workspace.drop_from_spaces(&b, cx);

        let space = workspace.active_space().expect("still open");
        assert_eq!(space.leaves(), 2);
        assert!(!space.contains(&b), "the pane went with the entry");
        assert_eq!(workspace.space_holding(&b), None);
    });
}

/// Taking a pane out down to one takes the space with it — the same rule
/// closing one by hand follows.
#[gpui::test]
fn dropping_a_pane_down_to_one_drops_the_space(cx: &mut gpui::TestAppContext) {
    let scratch = Scratch::new("last");
    let (workspace, a, b) = two_boards(&scratch, cx);

    workspace.update(cx, |workspace, cx| {
        workspace.arrange(&a, &b, Side::Right, cx);
        workspace.drop_from_spaces(&a, cx);

        assert!(workspace.spaces.is_empty(), "nothing left to arrange");
        assert_eq!(workspace.space_holding(&b), None, "and b is loose again");
    });
}

/// Sending into a session that is a pane of the open space leaves the window
/// where it was. Sending is not opening — the arrangement is the working
/// context, and a message must not throw it away.
#[gpui::test]
fn sending_into_a_pane_stays_in_the_space(cx: &mut gpui::TestAppContext) {
    let scratch = Scratch::new("send-in-space");
    cx.update(|cx| bezel::theme::Theme::install(bezel::theme::Appearance::Light, cx));
    // `send` refuses and says so while sessions are off, and would never reach
    // the bookkeeping under test.
    let mut settings = Settings::default();
    settings.features.sessions = true;
    let workspace = cx.new(|cx| Workspace::new(settings, state::State::default(), cx));

    workspace.update(cx, |workspace, cx| {
        workspace.open_project(scratch.project("one"), cx);
        workspace.new_board(0, "First".into(), "ONE", cx).ok();
        let board = workspace.board_member(0, 0).expect("a member");

        // Restored rather than opened: a session made through `new_session`
        // starts an agent, and what is under test is the bookkeeping `send`
        // does around one.
        let path = workspace.projects[0].path.clone();
        workspace.projects[0]
            .sessions
            .push(crate::model::project::HeldSession::new(
                resting_session(7, &path),
                cx,
            ));
        workspace.projects[0].active = Some(7);
        let chat = workspace
            .member_of(0, Showing::Session(7), cx)
            .expect("a member");

        workspace.arrange(&board, &chat, Side::Right, cx);
        assert!(workspace.active_space().is_some(), "arranged to start");

        workspace.send(7, "ship it".into(), cx);

        assert!(
            workspace.active_space().is_some(),
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

/// An entry moved to a tab of its own opens in the strip, and starts no
/// space.
#[gpui::test]
fn a_tab_of_its_own_starts_no_space(cx: &mut gpui::TestAppContext) {
    use crate::model::workspace::Opening;
    let scratch = Scratch::new("join-strip");
    let (workspace, a, b) = two_boards(&scratch, cx);
    workspace.update(cx, |workspace, cx| {
        workspace.open_tab(a.clone(), Opening::Kept, cx);
        workspace.join_strip(&b, cx);
        assert!(workspace.active_space().is_none(), "no space started");
        assert_eq!(workspace.tab_strip(), vec![a.clone(), b.clone()]);
        assert_eq!(
            workspace.front_slot().and_then(|slot| slot.entry()),
            Some(&b)
        );
    });
}

/// A drop on a pane but not on its edge mints a space of one pane over the
/// entry there, stacking the arrival in front of it; opening the entry behind
/// brings it back to the front.
#[gpui::test]
fn a_drop_off_the_edge_stacks_the_arrival(cx: &mut gpui::TestAppContext) {
    use crate::model::workspace::{Opening, Slot};
    let scratch = Scratch::new("stack");
    let (workspace, a, b) = two_boards(&scratch, cx);
    workspace.update(cx, |workspace, cx| {
        workspace.open_tab(a.clone(), Opening::Kept, cx);
        workspace.stack_pane(&a, &b, cx).expect("stacked");
        let space = workspace.active_space().expect("a space");
        assert_eq!(space.leaves(), 1, "one pane");
        assert_eq!(space.stack_of(&a), vec![a.clone(), b.clone()]);
        assert_eq!(space.front_of(&a), Some(b.clone()));
        let id = space.id.clone();
        assert_eq!(workspace.strip(), &[Slot::Space { space: id.clone() }]);

        workspace.set_front(&a, cx);
        let read = Shelf::read(&id)
            .expect("on disk")
            .map(&mut |path| workspace.intern(&path));
        assert_eq!(read.front_of(&b), Some(a.clone()), "written");
    });
}

/// An entry taken out of a stack into a pane of its own stands beside the
/// pane it left, which keeps the rest of the stack.
#[gpui::test]
fn a_stacked_entry_splits_out_beside_its_pane(cx: &mut gpui::TestAppContext) {
    use crate::model::workspace::Opening;
    let scratch = Scratch::new("split-out");
    let (workspace, a, b) = two_boards(&scratch, cx);
    workspace.update(cx, |workspace, cx| {
        workspace.open_tab(a.clone(), Opening::Kept, cx);
        workspace.stack_pane(&a, &b, cx).expect("stacked");
        workspace.relocate_pane(&b, &a, Side::Right, cx);
        let space = workspace.active_space().expect("a space");
        assert_eq!(space.leaves(), 2);
        assert_eq!(space.stack_of(&a), vec![a.clone()]);
        assert_eq!(space.tree.fronts(), vec![a.clone(), b.clone()]);
    });
}

/// A pane put on an article opens it. An article the sidebar has only listed
/// holds no editor, and a pane handed one draws the front door instead of the
/// document — which is what a drop onto a pane's bar or its edge used to do.
#[gpui::test]
fn a_pane_put_on_an_article_opens_it(cx: &mut gpui::TestAppContext) {
    let scratch = Scratch::new("article-pane");
    let path = scratch.project("one");
    // Written before the project is opened, so the article arrives the way one
    // read off disk at launch does: listed, and never opened.
    crate::model::article::create(&path).expect("an article on disk");

    cx.update(|cx| bezel::theme::Theme::install(bezel::theme::Appearance::Light, cx));
    let workspace = cx.new(|cx| Workspace::new(Settings::default(), state::State::default(), cx));

    workspace.update(cx, |workspace, cx| {
        workspace.open_project(path, cx);
        workspace.new_board(0, "First".into(), "ONE", cx).ok();
        let board = workspace.board_member(0, 0).expect("a member");
        let article = workspace.article_member(0, 0).expect("a member");
        assert!(
            workspace
                .article_in(0, 0)
                .expect("an article")
                .document
                .is_none(),
            "listed, never opened",
        );

        workspace.arrange(&board, &article, Side::Right, cx);
        let id = workspace.projects[0].articles[0].id.clone();
        workspace.select_showing(0, Showing::Article(id), cx);

        assert!(
            workspace
                .article_in(0, 0)
                .expect("an article")
                .document
                .is_some(),
            "the pane opened it",
        );
        assert_eq!(workspace.projects[0].article, Some(0));
    });
}

/// A launch lands back in the arrangement the window closed on. The space is
/// the window's, so it is written to `state.toml` rather than to any project.
#[gpui::test]
fn the_open_space_is_restored_at_launch(cx: &mut gpui::TestAppContext) {
    let scratch = Scratch::new("restore");
    let (workspace, a, b) = two_boards(&scratch, cx);

    let id = workspace.update(cx, |workspace, cx| {
        workspace.arrange(&a, &b, Side::Right, cx);
        workspace.active_space().expect("open").id.clone()
    });

    let stored = state::restore();
    assert_eq!(stored.space.as_deref(), Some(id.as_str()), "written down");

    let next = cx.new(|cx| Workspace::new(Settings::default(), stored, cx));
    next.update(cx, |workspace, _| {
        assert_eq!(
            workspace.active_space().map(|space| space.id.clone()),
            Some(id),
            "and opened again"
        );
    });
}

/// A single entry brought in front of the space is remembered too — the next
/// launch lands on that entry, not back in the space.
#[gpui::test]
fn leaving_the_space_is_remembered(cx: &mut gpui::TestAppContext) {
    let scratch = Scratch::new("restore-left");
    let (workspace, a, b) = two_boards(&scratch, cx);

    workspace.update(cx, |workspace, cx| {
        workspace.arrange(&a, &b, Side::Right, cx);
        workspace.new_board(0, "Third".into(), "THR", cx).ok();
        let c = workspace.board_member(0, 0).expect("a member");
        workspace.open_tab(c, crate::model::workspace::Opening::Kept, cx);
    });

    assert_eq!(state::restore().space, None);
}

/// A session that has had no turn has no file, and a space names its members
/// by file. Dropping one into an arrangement mints the file first — see
/// `Cydonia::dropped`, whose half of this is the gesture.
#[gpui::test]
fn a_session_with_no_file_can_be_given_one_and_arranged(cx: &mut gpui::TestAppContext) {
    let scratch = Scratch::new("fresh-session");
    cx.update(|cx| bezel::theme::Theme::install(bezel::theme::Appearance::Light, cx));
    let mut settings = Settings::default();
    settings.features.sessions = true;
    let workspace = cx.new(|cx| Workspace::new(settings, state::State::default(), cx));

    workspace.update(cx, |workspace, cx| {
        workspace.open_project(scratch.project("one"), cx);
        workspace.new_board(0, "First".into(), "ONE", cx).ok();
        let board = workspace.board_member(0, 0).expect("a member");

        let path = workspace.projects[0].path.clone();
        let mut chat = resting_session(7, &path);
        chat.record = None;
        workspace.projects[0]
            .sessions
            .push(crate::model::project::HeldSession::new(chat, cx));

        assert!(
            workspace.member_of_session(7, cx).is_none(),
            "nothing for a space to name yet",
        );

        workspace.retain_session(7, cx).expect("a file is minted");
        let session = workspace
            .member_of_session(7, cx)
            .expect("and now it names one");

        workspace.arrange(&board, &session, Side::Right, cx);
        let space = workspace.active_space().expect("arranged");
        assert!(space.contains(&session), "with the session in it");
    });
}

/// Closing a pane in a space of two. The space goes with it — one pane left
/// is no arrangement — and the window lands on the survivor.
#[gpui::test]
fn a_pane_closes(cx: &mut gpui::TestAppContext) {
    let scratch = Scratch::new("close-one-tab");
    let (workspace, a, b) = two_boards(&scratch, cx);

    workspace.update(cx, |workspace, cx| {
        workspace.arrange(&a, &b, Side::Right, cx);

        workspace.close_pane(&b, cx);
        assert!(workspace.active_space().is_none(), "the space goes too");
        assert!(
            workspace.active_board(cx).is_some(),
            "landing on the survivor"
        );
    });
}

/// A project's fold is the reader's: making an entry in it leaves it folded.
#[gpui::test]
fn making_an_entry_leaves_the_project_folded(cx: &mut gpui::TestAppContext) {
    let scratch = Scratch::new("reveal");
    cx.update(|cx| bezel::theme::Theme::install(bezel::theme::Appearance::Light, cx));
    let workspace = cx.new(|cx| Workspace::new(Settings::default(), state::State::default(), cx));

    workspace.update(cx, |workspace, cx| {
        workspace.open_project(scratch.project("one"), cx);
        workspace.toggle_project(0, cx);
        assert!(!workspace.projects[0].expanded, "folded to start");

        workspace.new_board(0, "First".into(), "ONE", cx).ok();
        assert!(!workspace.projects[0].expanded);
    });
}

/// Spaces are listed in hand order once one has been dragged in the sidebar,
/// and a re-read off disk keeps it — the files carry the time they were
/// written and nothing else.
#[gpui::test]
fn a_space_keeps_the_place_it_was_dragged_to(cx: &mut gpui::TestAppContext) {
    let scratch = Scratch::new("reorder");
    let (workspace, a, b) = two_boards(&scratch, cx);

    workspace.update(cx, |workspace, cx| {
        workspace.arrange(&a, &b, Side::Right, cx);
        // A second space, over two boards the first one does not hold: an
        // entry is in one space at a time.
        workspace.new_board(0, "Third".into(), "THR", cx).ok();
        workspace.new_board(0, "Fourth".into(), "FOU", cx).ok();
        let c = workspace.board_member(0, 0).expect("a member");
        let d = workspace.board_member(0, 1).expect("a member");
        workspace.open_tab(c.clone(), crate::model::workspace::Opening::Kept, cx);
        workspace.arrange(&c, &d, Side::Right, cx);

        let ids = |workspace: &Workspace| -> Vec<String> {
            workspace.space_tabs().map(str::to_owned).collect()
        };
        let open = |workspace: &Workspace| workspace.active_space().map(|space| space.id.clone());
        let before = ids(workspace);
        let front = open(workspace);

        workspace.move_space(&before[0], 1, cx);
        let moved = vec![before[1].clone(), before[0].clone()];
        assert_eq!(ids(workspace), moved);
        assert_eq!(open(workspace), front, "the open one is still open");
        let numbered: Vec<_> = workspace
            .numbered_tabs()
            .into_iter()
            .filter_map(|slot| slot.space().map(str::to_owned))
            .collect();
        assert_eq!(numbered, moved, "cmd-N counts the spaces in that order");

        let all = Shelf::all()
            .into_iter()
            .map(|space| space.map(&mut |path| workspace.intern(&path)))
            .collect();
        let reread: Vec<String> = Workspace::in_order(all, &moved)
            .into_iter()
            .map(|space| space.id)
            .collect();
        assert_eq!(reread, moved, "a re-read is not a re-sort");
    });
}

/// A space folds in the sidebar the way a project does, and a fold written to
/// `state.toml` is the fold the next launch starts with.
#[gpui::test]
fn a_space_fold_is_kept_like_a_project_fold(cx: &mut gpui::TestAppContext) {
    let scratch = Scratch::new("fold-space");
    let (workspace, a, b) = two_boards(&scratch, cx);

    let id = workspace.update(cx, |workspace, cx| {
        workspace.arrange(&a, &b, Side::Right, cx);
        let id = workspace.spaces[0].id.clone();
        assert!(!workspace.space_folded(&id), "open to start");
        workspace.toggle_space(&id, cx);
        assert!(workspace.space_folded(&id));
        id
    });

    let state = state::State {
        folded_spaces: vec![id.clone()],
        ..Default::default()
    };
    let restored = cx.new(|cx| Workspace::new(Settings::default(), state, cx));
    restored.update(cx, |workspace, _| assert!(workspace.space_folded(&id)));
}

/// A card written in a pane is filed into that pane's board, not into
/// whichever board its project is pointed at.
#[gpui::test]
fn a_pane_answers_for_its_own_board(cx: &mut gpui::TestAppContext) {
    let scratch = Scratch::new("pane-board");
    cx.update(|cx| bezel::theme::Theme::install(bezel::theme::Appearance::Light, cx));
    let workspace = cx.new(|cx| Workspace::new(Settings::default(), state::State::default(), cx));

    workspace.update(cx, |workspace, cx| {
        workspace.open_project(scratch.project("one"), cx);
        for (name, key) in [("First", "ONE"), ("Second", "TWO")] {
            workspace.new_board(0, name.into(), key, cx).ok();
        }
        let a = workspace.board_member(0, 0).expect("a member");
        let b = workspace.board_member(0, 1).expect("a member");
        workspace.arrange(&a, &b, Side::Right, cx);

        // Whatever the project is pointed at, each pane answers for itself.
        let on_a = workspace
            .board_of(Some(&a), cx)
            .expect("a board")
            .id
            .clone();
        let on_b = workspace
            .board_of(Some(&b), cx)
            .expect("a board")
            .id
            .clone();
        assert_ne!(on_a, on_b, "two panes, two boards");
        assert_eq!(
            workspace.board_of(None, cx).map(|board| board.id.clone()),
            workspace.active_board(cx).map(|board| board.id.clone()),
            "and no pane falls back to the project's own",
        );

        let column = workspace
            .write_board(&on_a, cx, |board| board.add_column("Todo").id.clone())
            .expect("the pane's board");
        workspace
            .write_board(&on_a, cx, |board| {
                board.add_card(&column, "written in the left pane".into());
            })
            .expect("the pane's board");

        assert_eq!(
            workspace
                .board_at(&on_a, cx)
                .and_then(|board| board.column(&column))
                .map(|column| column.cards.len()),
            Some(1),
        );
        assert!(
            workspace
                .board_at(&on_b, cx)
                .expect("still there")
                .columns
                .is_empty(),
            "and nothing landed in the other pane's board",
        );
    });
}

/// Another writer saving a board between this window's read and its write —
/// a terminal cydonia on the same project — keeps its change, and this
/// window's edit lands on top of it.
#[gpui::test]
fn a_stale_board_write_lands_on_the_other_writers_board(cx: &mut gpui::TestAppContext) {
    use artifact::project::Project as _;
    let scratch = Scratch::new("stale-board");
    cx.update(|cx| bezel::theme::Theme::install(bezel::theme::Appearance::Light, cx));
    let workspace = cx.new(|cx| Workspace::new(Settings::default(), state::State::default(), cx));

    workspace.update(cx, |workspace, cx| {
        let path = scratch.project("one");
        workspace.open_project(path.clone(), cx);
        workspace.new_board(0, "Plans".into(), "PLAN", cx).ok();
        let id = workspace.active_board(cx).expect("a board").id.clone();
        let column = workspace
            .write_board(&id, cx, |board| board.add_column("Todo").id.clone())
            .expect("the board");

        // The other writer, straight through the backend, unseen by the window.
        let other = crate::model::store::open(&path);
        let mut theirs = other.board(&id).expect("on disk");
        theirs.add_card(&column, "theirs".into());
        other.save_board(&mut theirs).expect("their save");

        workspace
            .write_board(&id, cx, |board| {
                board.add_card(&column, "mine".into());
            })
            .expect("the board");

        let texts = |board: &Board| -> Vec<String> {
            board
                .column(&column)
                .expect("the lane")
                .cards
                .iter()
                .map(|card| card.text.clone())
                .collect()
        };
        let saved = other.board(&id).expect("on disk");
        assert_eq!(texts(&saved), ["theirs", "mine"]);
        assert_eq!(
            texts(workspace.active_board(cx).expect("held")),
            ["theirs", "mine"]
        );
    });
}

/// A space is reached through its tab alone, so one `state.toml` holds no
/// tab for gets one at launch.
#[gpui::test]
fn every_space_has_a_tab_after_a_relaunch(cx: &mut gpui::TestAppContext) {
    let scratch = Scratch::new("space-tab");
    let (workspace, a, b) = two_boards(&scratch, cx);

    let id = workspace.update(cx, |workspace, cx| {
        workspace.arrange(&a, &b, Side::Right, cx);
        workspace.spaces[0].id.clone()
    });

    let state = state::State {
        tabs: Some(Vec::new()),
        ..Default::default()
    };
    let restored = cx.new(|cx| Workspace::new(Settings::default(), state, cx));
    restored.update(cx, |workspace, _| {
        assert!(
            workspace
                .strip()
                .contains(&crate::model::workspace::Slot::Space { space: id.clone() })
        )
    });
}

/// Archived anywhere, not only from the sidebar, an entry leaves its space,
/// and a space left with one pane goes with it.
#[gpui::test]
fn archiving_an_entry_takes_it_out_of_its_space(cx: &mut gpui::TestAppContext) {
    let scratch = Scratch::new("archived");
    let (workspace, a, b) = two_boards(&scratch, cx);

    workspace.update(cx, |workspace, cx| {
        workspace.new_board(0, "Third".into(), "THR", cx).ok();
        let c = workspace.board_member(0, 0).expect("a member");
        workspace.arrange(&a, &b, Side::Right, cx);
        workspace.arrange(&b, &c, Side::Below, cx);

        workspace.archive_board(b.id(), true, cx);
        let space = workspace.active_space().expect("still open");
        assert!(!space.contains(&b), "the pane went with the entry");
        assert_eq!(space.leaves(), 2);

        workspace.archive_board(c.id(), true, cx);
        assert!(
            workspace.spaces.is_empty(),
            "one pane left is no arrangement"
        );
        assert_eq!(workspace.space_holding(&a), None);
    });
}

/// An app is held by the workspace, not by a project: developer mode off
/// stops listing a loaded one, and its place in a space stays.
#[gpui::test]
fn a_loaded_app_keeps_its_place_with_developer_mode_off(cx: &mut gpui::TestAppContext) {
    let scratch = Scratch::new("app");
    // SAFETY: as for the config directory above.
    unsafe { std::env::set_var("XDG_DATA_HOME", scratch.0.join("data")) };
    artifact::app::Loaded {
        id: "hello".into(),
        path: scratch.project("hello"),
    }
    .save()
    .unwrap();
    cx.update(|cx| bezel::theme::Theme::install(bezel::theme::Appearance::Light, cx));
    let settings = Settings {
        developer_mode: true,
        ..Settings::default()
    };
    let workspace = cx.new(|cx| Workspace::new(settings, state::State::default(), cx));
    workspace.update(cx, |workspace, cx| {
        workspace.open_project(scratch.project("one"), cx);
        workspace.new_board(0, "First".into(), "ONE", cx).ok();
        let board = workspace.board_member(0, 0).expect("a member");
        let app = Member::app("hello");
        assert_eq!(workspace.listed_apps(cx).count(), 1);
        assert_eq!(workspace.arrange(&board, &app, Side::Right, cx), Some(0));

        workspace.set_developer_mode(false, cx);
        workspace.reload_apps(cx);
        workspace.prune_spaces(cx);
        assert_eq!(workspace.listed_apps(cx).count(), 0);
        assert!(workspace.program("hello", cx).is_none());
        assert!(workspace.active_space().expect("open").contains(&app));
    });
}

// ── the window's strip ──────────────────────────────────────────

/// Opened as a preview, an entry takes the preview tab's place; kept, it lands
/// beside the tab in front and leaves the preview where it is.
#[gpui::test]
fn a_preview_is_replaced_and_a_kept_tab_is_not(cx: &mut gpui::TestAppContext) {
    use crate::model::workspace::Opening;
    let scratch = Scratch::new("tab-preview");
    let (workspace, a, b) = two_boards(&scratch, cx);
    workspace.update(cx, |workspace, cx| {
        workspace.new_board(0, "Third".into(), "THREE", cx).ok();
        let c = workspace.board_member(0, 0).expect("a member");

        workspace.open_tab(a.clone(), Opening::Preview, cx);
        workspace.open_tab(b.clone(), Opening::Preview, cx);
        assert_eq!(workspace.tab_strip(), vec![b.clone()], "b took a's place");
        assert_eq!(workspace.preview_tab(), Some(&b));

        workspace.open_tab(c.clone(), Opening::Kept, cx);
        assert_eq!(workspace.tab_strip(), vec![b.clone(), c.clone()]);
        assert_eq!(workspace.preview_tab(), Some(&b), "the preview stays one");

        workspace.keep_tab(&b, cx);
        workspace.open_tab(a.clone(), Opening::Preview, cx);
        assert_eq!(
            workspace.tab_strip(),
            vec![b.clone(), c.clone(), a.clone()],
            "with no preview left, a lands after the tab in front"
        );
    });
}

/// An entry written to while it is the preview is kept.
#[gpui::test]
fn an_edited_preview_is_kept(cx: &mut gpui::TestAppContext) {
    use crate::model::workspace::Opening;
    let scratch = Scratch::new("tab-edited");
    let (workspace, a, _) = two_boards(&scratch, cx);
    workspace.update(cx, |workspace, cx| {
        workspace.open_tab(a.clone(), Opening::Preview, cx);
        workspace.write_board(a.id(), cx, |board| board.add_column("Doing").id.clone());
        assert_eq!(workspace.preview_tab(), None);
    });
}

/// Entries put into a space leave the strip for the space's tab, and closing
/// that tab deletes the space and leaves the window on none.
#[gpui::test]
fn tabs_close_and_leave_for_a_space(cx: &mut gpui::TestAppContext) {
    use crate::model::workspace::Opening;
    let scratch = Scratch::new("tab-close");
    let (workspace, a, b) = two_boards(&scratch, cx);
    workspace.update(cx, |workspace, cx| {
        workspace.open_tab(a.clone(), Opening::Kept, cx);
        workspace.open_tab(b.clone(), Opening::Kept, cx);
        workspace.arrange(&a, &b, Side::Right, cx);
        assert!(workspace.tab_strip().is_empty(), "both went to the space");

        let id = workspace.active_space().expect("open").id.clone();
        workspace.delete_space_id(&id, cx);
        assert!(workspace.strip().is_empty());
        assert!(workspace.arranged().is_none());
        assert!(
            workspace.spaces.is_empty(),
            "closing its tab deletes the space"
        );
    });
}

/// A space made over the entry in front takes that entry's place in the
/// strip, and closed down to one entry it hands its place to the survivor.
#[gpui::test]
fn a_space_is_a_tab_of_the_strip(cx: &mut gpui::TestAppContext) {
    use crate::model::workspace::{Opening, Slot};
    let scratch = Scratch::new("space-tab");
    let (workspace, a, b) = two_boards(&scratch, cx);
    workspace.update(cx, |workspace, cx| {
        workspace.new_board(0, "Third".into(), "THR", cx).ok();
        let c = workspace.board_member(0, 0).expect("a member");
        workspace.open_tab(a.clone(), Opening::Kept, cx);
        workspace.open_tab(c.clone(), Opening::Kept, cx);
        workspace.front_tab(&Slot::Entry(a.clone()), cx);

        workspace.arrange(&a, &b, Side::Right, cx);
        let id = workspace.active_space().expect("open").id.clone();
        assert_eq!(
            workspace.strip(),
            [Slot::Space { space: id.clone() }, Slot::Entry(c.clone())]
        );

        workspace.close_pane(&b, cx);
        assert!(workspace.spaces.is_empty(), "one entry is no arrangement");
        assert_eq!(
            workspace.strip(),
            [Slot::Entry(a.clone()), Slot::Entry(c.clone())]
        );
        assert_eq!(workspace.front_slot(), Some(&Slot::Entry(a.clone())));
    });
}
