use super::*;
use crate::model::{project::Project, settings::Settings, state};
use artifact::project::{Project as _, fs};
use bezel::gpui::{Modifiers, MouseButton, TestAppContext, VisualTestContext, point, size};

struct Scratch(std::path::PathBuf);
impl Scratch {
    fn new(name: &str) -> Self {
        let path =
            std::env::temp_dir().join(format!("cydonia-drawer-{name}-{}", std::process::id()));
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

struct BoardView(Entity<Cydonia>);
impl Render for BoardView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div().size_full().flex().flex_col().child(
            self.0
                .update(cx, |root, cx| root.board(0, 0, None, window, cx)),
        )
    }
}

fn open(
    name: &str,
    cx: &mut TestAppContext,
) -> (
    Scratch,
    Entity<Cydonia>,
    Member,
    Vec<String>,
    VisualTestContext,
) {
    let scratch = Scratch::new(name);
    let store = fs::Project::new(&scratch.0);
    let mut board = store.create_board("Work", "DEV").unwrap();
    let column = board.add_column("Todo").id.clone();
    let cards = vec![
        board
            .add_card(&column, "First card".into())
            .unwrap()
            .id
            .clone(),
        board
            .add_card(&column, "Second card".into())
            .unwrap()
            .id
            .clone(),
    ];
    store.save_board(&mut board);
    let member = Member::new(&scratch.0, artifact::space::Kind::Board, board.id.clone());
    cx.update(|cx| {
        Theme::install(bezel::theme::Appearance::Dark, cx);
        input::init(cx);
        cx.bind_keys(bindings());
    });
    let window = cx.add_window(|window, cx| {
        let root =
            cx.new(|cx| Cydonia::new(Settings::default(), state::State::default(), window, cx));
        root.update(cx, |root, cx| {
            root.workspace.update(cx, |workspace, _| {
                let mut project = Project::new(scratch.0.clone());
                project.load_board(&board.id);
                project.board = Some(0);
                workspace.projects.push(project);
                workspace.active = Some(0);
            });
            root.open_card(None, member.clone(), cards[0].clone(), window, cx);
        });
        cx.observe(&root, |_, _, cx| cx.notify()).detach();
        BoardView(root)
    });
    let root = window
        .root(cx)
        .unwrap()
        .read_with(cx, |view, _| view.0.clone());
    let mut visual = VisualTestContext::from_window(window.into(), cx);
    visual.simulate_resize(size(px(700.), px(600.)));
    visual.update(|window, _| window.refresh());
    settle(&mut visual);
    (scratch, root, member, cards, visual)
}

fn settle(cx: &mut VisualTestContext) {
    // Deliver layout follow-up frames; hover/caret animations may keep scheduling.
    for _ in 0..4 {
        cx.run_until_parked();
        if cx.update(|window, cx| window.simulate_next_frame(cx)) == 0 {
            return;
        }
    }
}

fn click(selector: &'static str, cx: &mut VisualTestContext) {
    let center = cx.debug_bounds(selector).unwrap().center();
    cx.simulate_mouse_down(center, MouseButton::Left, Modifiers::default());
    cx.simulate_mouse_up(center, MouseButton::Left, Modifiers::default());
    settle(cx);
}

#[gpui::test]
fn resizing_expanding_and_restoring_use_the_board_pane(cx: &mut TestAppContext) {
    let (_scratch, root, _, _, mut cx) = open("resize", cx);
    let handle = cx.debug_bounds("card-drawer-resize").unwrap().center();
    cx.simulate_mouse_down(handle, MouseButton::Left, Modifiers::default());
    cx.simulate_mouse_move(
        handle - point(px(0.), px(20.)),
        Some(MouseButton::Left),
        Modifiers::default(),
    );
    assert!(!cx.update(|_, cx| cx.has_active_drag()));
    assert!(cx.update(|_, cx| {
        root.read(cx)
            .leaf()
            .open_card
            .as_ref()
            .unwrap()
            .resize_grab
            .is_some()
    }));
    cx.simulate_mouse_move(
        point(handle.x, px(180.)),
        Some(MouseButton::Left),
        Modifiers::default(),
    );
    cx.simulate_mouse_up(
        point(handle.x, px(180.)),
        MouseButton::Left,
        Modifiers::default(),
    );
    settle(&mut cx);
    let resized = cx.debug_bounds("card-drawer").unwrap().size.height;
    assert!(resized > px(380.) && resized < px(460.), "{resized:?}");
    click("card-drawer-expand", &mut cx);
    assert_eq!(
        cx.debug_bounds("card-drawer").unwrap().size.height,
        px(600.)
    );
    click("card-drawer-expand", &mut cx);
    assert_eq!(cx.debug_bounds("card-drawer").unwrap().size.height, resized);
    cx.simulate_resize(size(px(700.), px(120.)));
    settle(&mut cx);
    assert!(cx.debug_bounds("card-drawer").unwrap().size.height <= px(120.));
    assert!(!cx.update(|_, cx| {
        root.read(cx)
            .leaf()
            .open_card
            .as_ref()
            .unwrap()
            .size
            .expanded
    }));
    cx.simulate_resize(size(px(240.), px(400.)));
    settle(&mut cx);
    click("card-drawer-edit", &mut cx);
    for selector in [
        "card-draft-save",
        "card-draft-cancel",
        "card-drawer-expand",
        "card-drawer-close",
    ] {
        let bounds = cx.debug_bounds(selector).unwrap();
        assert!(
            bounds.left() >= px(0.) && bounds.right() <= px(240.),
            "{selector}: {bounds:?}"
        );
    }
}

#[gpui::test]
fn drafts_survive_preview_switching_cards_and_closing(cx: &mut TestAppContext) {
    let (scratch, root, member, cards, mut cx) = open("draft", cx);
    click("card-drawer-edit", &mut cx);
    cx.update(|_, cx| {
        let field = root.read(cx).draft_for(None).unwrap().field.clone();
        field.update(cx, |field, cx| field.set_content("Draft text", cx));
    });
    settle(&mut cx);
    click("card-drawer-edit", &mut cx);
    cx.update(|window, cx| {
        root.update(cx, |root, cx| {
            root.open_card(None, member.clone(), cards[1].clone(), window, cx);
            assert!(root.draft_for(None).is_none());
            root.open_card(None, member.clone(), cards[0].clone(), window, cx);
            assert_eq!(
                root.draft_for(None)
                    .unwrap()
                    .field
                    .read(cx)
                    .content()
                    .as_ref(),
                "Draft text"
            );
            root.close_card_preview(None, window, cx);
            root.open_card(None, member.clone(), cards[0].clone(), window, cx);
            assert_eq!(
                root.draft_for(None)
                    .unwrap()
                    .field
                    .read(cx)
                    .content()
                    .as_ref(),
                "Draft text"
            );
        })
    });
    assert_eq!(
        fs::Project::new(&scratch.0)
            .board(&member.id)
            .unwrap()
            .card(&cards[0])
            .unwrap()
            .text,
        "First card"
    );
    settle(&mut cx);
    click("card-draft-save", &mut cx);
    assert_eq!(
        fs::Project::new(&scratch.0)
            .board(&member.id)
            .unwrap()
            .card(&cards[0])
            .unwrap()
            .text,
        "Draft text"
    );
    assert!(cx.update(|_, cx| root.read(cx).draft_for(None).is_none()));
}

#[gpui::test]
fn save_preserves_agent_metadata_and_rejects_conflicting_text(cx: &mut TestAppContext) {
    let (scratch, root, member, cards, mut cx) = open("conflict", cx);
    click("card-drawer-edit", &mut cx);
    cx.update(|_, cx| {
        let field = root.read(cx).draft_for(None).unwrap().field.clone();
        field.update(cx, |field, cx| field.set_content("My draft", cx));
    });
    let store = fs::Project::new(&scratch.0);
    let mut board = store.board(&member.id).unwrap();
    board.set_card_status(&cards[0], Some(Status::Busy));
    board.card_mut(&cards[0]).unwrap().session = Some("agent-session".into());
    store.save_board(&mut board);
    cx.update(|window, cx| root.update(cx, |root, cx| root.save_card_draft(None, window, cx)));
    let saved = store.board(&member.id).unwrap();
    assert_eq!(saved.card(&cards[0]).unwrap().status, Some(Status::Busy));
    assert_eq!(
        saved.card(&cards[0]).unwrap().session.as_deref(),
        Some("agent-session")
    );

    cx.update(|window, cx| {
        root.update(cx, |root, cx| {
            root.toggle_card_edit(None, window, cx);
            root.draft_for(None)
                .unwrap()
                .field
                .clone()
                .update(cx, |field, cx| field.set_content("New draft", cx));
        })
    });
    let mut board = store.board(&member.id).unwrap();
    board.rewrite_card(&cards[0], "Agent rewrite");
    store.save_board(&mut board);
    cx.update(|window, cx| {
        root.update(cx, |root, cx| {
            root.save_card_draft(None, window, cx);
            assert!(root.draft_for(None).unwrap().error.is_some());
            assert_eq!(
                root.draft_for(None)
                    .unwrap()
                    .field
                    .read(cx)
                    .content()
                    .as_ref(),
                "New draft"
            );
            root.cancel_card_draft(None, window, cx);
            assert!(root.draft_for(None).is_none());
        })
    });
    assert_eq!(
        store
            .board(&member.id)
            .unwrap()
            .card(&cards[0])
            .unwrap()
            .text,
        "Agent rewrite"
    );
}

#[gpui::test]
fn long_drafts_scroll_in_the_drawer_and_enter_does_not_save(cx: &mut TestAppContext) {
    let (scratch, root, member, cards, mut cx) = open("long-edit", cx);
    click("card-drawer-edit", &mut cx);
    cx.update(|_, cx| {
        let field = root.read(cx).draft_for(None).unwrap().field.clone();
        field.update(cx, |field, cx| field.set_content("line\n".repeat(700), cx));
    });
    settle(&mut cx);
    cx.update(|_, cx| {
        let root = root.read(cx);
        let opened = root.leaf().open_card.as_ref().unwrap();
        assert!(opened.scroll.max_offset().y > px(7000.));
        let field = root.draft_for(None).unwrap().field.read(cx);
        let caret = field.offset_bounds(field.cursor()).unwrap();
        assert!(caret.top() >= opened.scroll.bounds().top());
        assert!(
            caret.bottom() <= opened.scroll.bounds().bottom(),
            "caret={caret:?}, viewport={:?}, offset={:?}, max={:?}",
            opened.scroll.bounds(),
            opened.scroll.offset(),
            opened.scroll.max_offset()
        );
    });
    cx.simulate_keystrokes("enter");
    cx.simulate_input("last line");
    settle(&mut cx);
    assert_eq!(
        fs::Project::new(&scratch.0)
            .board(&member.id)
            .unwrap()
            .card(&cards[0])
            .unwrap()
            .text,
        "First card"
    );
    cx.simulate_keystrokes("cmd-enter");
    settle(&mut cx);
    let saved = fs::Project::new(&scratch.0).board(&member.id).unwrap();
    assert!(
        saved
            .card(&cards[0])
            .unwrap()
            .text
            .ends_with("\n\nlast line")
    );
    assert!(cx.update(|_, cx| root.read(cx).draft_for(None).is_none()));
}

#[gpui::test]
fn escape_keeps_the_draft_and_empty_saves_do_not_delete_cards(cx: &mut TestAppContext) {
    let (scratch, root, member, cards, mut cx) = open("escape", cx);
    click("card-drawer-edit", &mut cx);
    cx.update(|_, cx| {
        let field = root.read(cx).draft_for(None).unwrap().field.clone();
        field.update(cx, |field, cx| field.set_content("", cx));
    });
    settle(&mut cx);
    cx.simulate_keystrokes("cmd-enter");
    settle(&mut cx);
    assert!(cx.update(|_, cx| root.read(cx).draft_for(None).unwrap().error.is_some()));
    assert!(
        fs::Project::new(&scratch.0)
            .board(&member.id)
            .unwrap()
            .card(&cards[0])
            .is_some()
    );
    cx.simulate_keystrokes("escape");
    settle(&mut cx);
    cx.update(|window, cx| {
        root.update(cx, |root, cx| {
            assert!(root.leaf().open_card.is_none());
            root.open_card(None, member.clone(), cards[0].clone(), window, cx);
            assert!(
                root.draft_for(None)
                    .unwrap()
                    .field
                    .read(cx)
                    .content()
                    .is_empty()
            );
            root.cancel_card_draft(None, window, cx);
        })
    });
}

#[gpui::test]
fn a_removed_card_keeps_its_draft_accessible(cx: &mut TestAppContext) {
    let (scratch, root, member, cards, mut cx) = open("removed", cx);
    click("card-drawer-edit", &mut cx);
    cx.update(|_, cx| {
        root.read(cx)
            .draft_for(None)
            .unwrap()
            .field
            .clone()
            .update(cx, |field, cx| field.set_content("Keep this draft", cx));
    });
    let store = fs::Project::new(&scratch.0);
    let mut board = store.board(&member.id).unwrap();
    board.remove_card(&cards[0]);
    store.save_board(&mut board);
    cx.update(|window, cx| root.update(cx, |root, cx| root.save_card_draft(None, window, cx)));
    settle(&mut cx);
    assert!(cx.debug_bounds("card-drawer").is_some());
    cx.update(|_, cx| {
        assert_eq!(
            root.read(cx)
                .draft_for(None)
                .unwrap()
                .field
                .read(cx)
                .content()
                .as_ref(),
            "Keep this draft"
        );
    });
    assert!(store.board(&member.id).unwrap().card(&cards[0]).is_none());
}

#[gpui::test]
fn horizontal_scroll_keeps_offsets_and_lane_geometry_stable(cx: &mut TestAppContext) {
    let (_scratch, root, member, cards, mut cx) = open("horizontal", cx);
    cx.update(|_, cx| markdown::set_layout(cx, markdown::Layout { wrap_code: false }));
    cx.update(|window, cx| {
        root.update(cx, |root, cx| {
            root.close_card_preview(None, window, cx);
            root.workspace.update(cx, |workspace, cx| {
                let board = &mut workspace.projects[0].boards[0];
                for i in 0..4 {
                    let column = board.add_column(&format!("Lane {i}")).id.clone();
                    for _ in 0..6 {
                        board.add_card(&column, format!("```\n{}\n```", "long_code ".repeat(20)));
                    }
                }
                cx.notify();
            });
            cx.notify();
        });
    });
    settle(&mut cx);
    let scroll = cx.update(|_, cx| root.read(cx).boards.of(&member.id).across.clone());
    let max = scroll.max_offset();
    for y in (60..160).step_by(10) {
        scroll.set_offset(point(px(0.), px(0.)));
        cx.update(|window, _| window.refresh());
        settle(&mut cx);
        cx.simulate_event(gpui::ScrollWheelEvent {
            position: point(px(350.), px(y as f32)),
            delta: gpui::ScrollDelta::Pixels(point(px(-30.), px(0.))),
            modifiers: Default::default(),
            touch_phase: gpui::TouchPhase::Started,
        });
        settle(&mut cx);
        assert_eq!(scroll.offset().x, px(-30.), "y={y}");
        assert_eq!(scroll.max_offset(), max);
    }
    let lane = cx.update(|_, cx| {
        let root = root.read(cx);
        let id = &root.workspace.read(cx).projects[0].boards[0].columns[1].id;
        root.boards.of(&member.id).lanes.of(id).0
    });
    let horizontal = scroll.offset();
    cx.simulate_event(gpui::ScrollWheelEvent {
        position: point(px(350.), px(90.)),
        delta: gpui::ScrollDelta::Pixels(point(px(1.), px(-30.))),
        modifiers: Default::default(),
        touch_phase: gpui::TouchPhase::Started,
    });
    settle(&mut cx);
    assert_eq!(scroll.offset(), horizontal);
    assert_eq!(lane.offset().y, px(-30.));
    cx.update(|window, cx| {
        root.update(cx, |root, cx| {
            root.open_card(None, member.clone(), cards[0].clone(), window, cx)
        });
        scrollbars::set_visibility(scrollbars::Visibility::Always, cx);
    });
    settle(&mut cx);
    let before = scroll.offset();
    cx.simulate_event(gpui::ScrollWheelEvent {
        position: point(px(350.), px(450.)),
        delta: gpui::ScrollDelta::Pixels(point(px(-30.), px(0.))),
        modifiers: Default::default(),
        touch_phase: gpui::TouchPhase::Started,
    });
    settle(&mut cx);
    assert_eq!(
        scroll.offset(),
        before,
        "drawer wheel must not move the board"
    );
    assert!(cx.debug_bounds("board-bar-track").is_some());
    let grip = cx.debug_bounds("card-drawer-resize").unwrap().center();
    cx.simulate_mouse_down(grip, MouseButton::Left, Modifiers::default());
    cx.simulate_mouse_move(
        grip - point(px(0.), px(50.)),
        MouseButton::Left,
        Modifiers::default(),
    );
    settle(&mut cx);
    assert!(cx.debug_bounds("board-bar-track").is_none());
    assert!(!cx.update(|_, cx| cx.has_active_drag()));
    cx.simulate_mouse_up(
        grip - point(px(0.), px(50.)),
        MouseButton::Left,
        Modifiers::default(),
    );
    settle(&mut cx);
    assert!(cx.debug_bounds("board-bar-track").is_some());
}

#[gpui::test]
fn offscreen_lanes_do_not_build_markdown(cx: &mut TestAppContext) {
    let (_scratch, root, member, _, mut cx) = open("offscreen", cx);
    cx.update(|window, cx| {
        root.update(cx, |root, cx| {
            root.close_card_preview(None, window, cx);
            root.workspace.update(cx, |workspace, _| {
                let board = &mut workspace.projects[0].boards[0];
                for i in 0..20 {
                    let column = board.add_column(&format!("Lane {i}")).id.clone();
                    for j in 0..5 {
                        board.add_card(
                            &column,
                            format!("Lane {i} card {j}\n\n{}", "Paragraph.\n\n".repeat(20)),
                        );
                    }
                }
            });
            cx.notify();
        })
    });
    settle(&mut cx);
    let scroll = cx.update(|_, cx| root.read(cx).boards.of(&member.id).across.clone());
    let max = scroll.max_offset();
    let parsed = cx.update(|_, cx| root.read(cx).card_docs.0.borrow().len());
    assert!(
        parsed <= 22,
        "built {parsed} documents for a 700px viewport"
    );
    let lane = cx.update(|_, cx| {
        let root = root.read(cx);
        let id = &root.workspace.read(cx).projects[0].boards[0].columns[1].id;
        root.boards.of(&member.id).lanes.of(id).0
    });
    lane.set_offset(point(px(0.), px(-120.)));
    cx.update(|window, _| window.refresh());
    settle(&mut cx);
    let lane_before = lane.offset();
    scroll.set_offset(point(px(-10. * COLUMN_WIDTH), px(0.)));
    cx.update(|window, _| window.refresh());
    settle(&mut cx);
    assert_eq!(scroll.max_offset(), max);
    assert!(cx.update(|_, cx| {
        root.read(cx)
            .card_docs
            .0
            .borrow()
            .keys()
            .any(|text| text.starts_with("Lane 10 card 0"))
    }));
    let parsed = cx.update(|_, cx| root.read(cx).card_docs.0.borrow().len());
    assert!(
        parsed <= 47,
        "built {parsed} documents after jumping across the board"
    );
    scroll.set_offset(point(px(0.), px(0.)));
    cx.update(|window, _| window.refresh());
    settle(&mut cx);
    assert_eq!(
        lane.offset(),
        lane_before,
        "remounting a lane preserves its position"
    );
    assert_eq!(scroll.max_offset(), max);
    cx.simulate_resize(size(px(1800.), px(600.)));
    settle(&mut cx);
    assert!(
        cx.update(|_, cx| root
            .read(cx)
            .card_docs
            .0
            .borrow()
            .keys()
            .any(|text| text.starts_with("Lane 6 card 0"))),
        "growing the viewport builds newly visible lanes"
    );
}
