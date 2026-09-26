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
    store.save_board(&mut board).unwrap();
    let member = Member::new(&scratch.0, artifact::space::Kind::Board, board.id.clone());
    cx.update(|cx| {
        Theme::install(bezel::theme::Appearance::Dark, cx);
        input::init(cx);
        editor::init(cx);
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

/// Replace the open card's text the way a person would: select all, type.
fn type_card(root: &Entity<Cydonia>, text: &str, cx: &mut VisualTestContext) {
    cx.update(|window, cx| {
        let editor = root.read(cx).draft_for(None).unwrap().editor.clone();
        window.focus(&editor.focus_handle(cx), cx);
    });
    cx.simulate_keystrokes("cmd-a");
    match text {
        "" => cx.simulate_keystrokes("backspace"),
        text => cx.simulate_input(text),
    }
    settle(cx);
}

fn draft_text(root: &Entity<Cydonia>, cx: &mut VisualTestContext) -> String {
    cx.update(|_, cx| {
        root.read(cx)
            .draft_for(None)
            .unwrap()
            .editor
            .read(cx)
            .source()
            .trim_end()
            .to_owned()
    })
}

fn saved_text(scratch: &Scratch, member: &Member, card: &str) -> Option<String> {
    fs::Project::new(&scratch.0)
        .board(&member.id)
        .unwrap()
        .card(card)
        .map(|card| card.text.trim_end().to_owned())
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
    for selector in ["card-drawer-expand", "card-drawer-close"] {
        let bounds = cx.debug_bounds(selector).unwrap();
        assert!(
            bounds.left() >= px(0.) && bounds.right() <= px(240.),
            "{selector}: {bounds:?}"
        );
    }
}

#[gpui::test]
fn typing_saves_the_card(cx: &mut TestAppContext) {
    let (scratch, root, member, cards, mut cx) = open("autosave", cx);
    type_card(&root, "Typed text", &mut cx);
    assert_eq!(
        saved_text(&scratch, &member, &cards[0]).unwrap(),
        "Typed text"
    );
}

#[gpui::test]
fn opening_a_card_writes_nothing(cx: &mut TestAppContext) {
    let (scratch, root, member, cards, mut cx) = open("untouched", cx);
    cx.update(|window, cx| {
        root.update(cx, |root, cx| {
            root.open_card(None, member.clone(), cards[1].clone(), window, cx);
            root.close_card_preview(None, window, cx);
        })
    });
    assert_eq!(
        saved_text(&scratch, &member, &cards[0]).unwrap(),
        "First card"
    );
    assert_eq!(
        saved_text(&scratch, &member, &cards[1]).unwrap(),
        "Second card"
    );
}

#[gpui::test]
fn switching_and_closing_save_at_once(cx: &mut TestAppContext) {
    let (scratch, root, member, cards, mut cx) = open("switch", cx);
    type_card(&root, "Switched away", &mut cx);
    cx.update(|window, cx| {
        root.update(cx, |root, cx| {
            root.open_card(None, member.clone(), cards[1].clone(), window, cx)
        })
    });
    assert_eq!(
        saved_text(&scratch, &member, &cards[0]).unwrap(),
        "Switched away"
    );
    assert_eq!(draft_text(&root, &mut cx), "Second card");
    type_card(&root, "Closed", &mut cx);
    click("card-drawer-close", &mut cx);
    assert_eq!(saved_text(&scratch, &member, &cards[1]).unwrap(), "Closed");
    assert!(cx.update(|_, cx| root.read(cx).leaf().card_drafts.is_empty()));
}

#[gpui::test]
fn save_preserves_agent_metadata_and_rejects_conflicting_text(cx: &mut TestAppContext) {
    let (scratch, root, member, cards, mut cx) = open("conflict", cx);
    type_card(&root, "My draft", &mut cx);
    let store = fs::Project::new(&scratch.0);
    let mut board = store.board(&member.id).unwrap();
    board.set_card_status(&cards[0], Some(Status::Busy));
    board.card_mut(&cards[0]).unwrap().session = Some("agent-session".into());
    store.save_board(&mut board).unwrap();
    type_card(&root, "Mine", &mut cx);
    let saved = store.board(&member.id).unwrap();
    assert_eq!(saved.card(&cards[0]).unwrap().text.trim_end(), "Mine");
    assert_eq!(saved.card(&cards[0]).unwrap().status, Some(Status::Busy));
    assert_eq!(
        saved.card(&cards[0]).unwrap().session.as_deref(),
        Some("agent-session")
    );

    let mut board = store.board(&member.id).unwrap();
    board.rewrite_card(&cards[0], "Agent rewrite");
    store.save_board(&mut board).unwrap();
    type_card(&root, "New draft", &mut cx);
    assert!(cx.update(|_, cx| root.read(cx).draft_for(None).unwrap().error.is_some()));
    assert_eq!(draft_text(&root, &mut cx), "New draft");
    assert_eq!(
        saved_text(&scratch, &member, &cards[0]).unwrap(),
        "Agent rewrite"
    );
    click("card-draft-discard", &mut cx);
    assert_eq!(draft_text(&root, &mut cx), "Agent rewrite");
    assert!(cx.update(|_, cx| root.read(cx).draft_for(None).unwrap().error.is_none()));
}

#[gpui::test]
fn an_agent_rewrite_reloads_an_untouched_card(cx: &mut TestAppContext) {
    let (_scratch, root, _, cards, mut cx) = open("reload", cx);
    cx.update(|_, cx| {
        root.update(cx, |root, cx| {
            root.workspace.update(cx, |workspace, cx| {
                workspace.projects[0].boards[0].rewrite_card(&cards[0], "Agent rewrite");
                cx.notify();
            });
            cx.notify();
        })
    });
    settle(&mut cx);
    assert_eq!(draft_text(&root, &mut cx), "Agent rewrite");
}

#[gpui::test]
fn emptied_cards_are_kept_and_never_saved(cx: &mut TestAppContext) {
    let (scratch, root, member, cards, mut cx) = open("empty", cx);
    type_card(&root, "", &mut cx);
    assert!(cx.update(|_, cx| root.read(cx).draft_for(None).unwrap().error.is_some()));
    assert_eq!(
        saved_text(&scratch, &member, &cards[0]).unwrap(),
        "First card"
    );
    click("card-drawer-close", &mut cx);
    cx.update(|window, cx| {
        root.update(cx, |root, cx| {
            assert!(root.leaf().open_card.is_none());
            root.open_card(None, member.clone(), cards[0].clone(), window, cx);
        })
    });
    assert_eq!(draft_text(&root, &mut cx), "");
}

#[gpui::test]
fn a_removed_card_keeps_its_text_accessible(cx: &mut TestAppContext) {
    let (scratch, root, member, cards, mut cx) = open("removed", cx);
    let store = fs::Project::new(&scratch.0);
    let mut board = store.board(&member.id).unwrap();
    board.remove_card(&cards[0]);
    store.save_board(&mut board).unwrap();
    type_card(&root, "Keep this text", &mut cx);
    assert!(cx.debug_bounds("card-drawer").is_some());
    assert_eq!(draft_text(&root, &mut cx), "Keep this text");
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

#[gpui::test]
fn list_scroll_builds_visible_rows_and_keeps_its_extent(cx: &mut TestAppContext) {
    let (_scratch, root, member, cards, mut cx) = open("list-window", cx);
    cx.update(|window, cx| {
        root.update(cx, |root, cx| {
            root.close_card_preview(None, window, cx);
            root.workspace.update(cx, |workspace, _| {
                let board = &mut workspace.projects[0].boards[0];
                board.view = View::List;
                let column = board.columns[0].id.clone();
                for i in 0..300 {
                    board.add_card(
                        &column,
                        format!("List row {i}\n\n{}", "Long body\n".repeat(100)),
                    );
                }
            });
            cx.notify();
        })
    });
    settle(&mut cx);
    let scroll = cx.update(|_, cx| root.read(cx).boards.of(&member.id).down.clone());
    let max = scroll.max_offset();
    assert!(max.y > px(10000.));
    let parsed = cx.update(|_, cx| root.read(cx).card_docs.0.borrow().len());
    assert!(
        parsed < 25,
        "built {parsed} documents for the first viewport"
    );
    scroll.set_offset(point(px(0.), px(-3600.)));
    cx.update(|window, _| window.refresh());
    settle(&mut cx);
    assert_eq!(scroll.max_offset(), max);
    assert_eq!(scroll.offset().y, px(-3600.));
    assert!(cx.update(|_, cx| {
        root.read(cx)
            .card_docs
            .0
            .borrow()
            .keys()
            .any(|text| text.starts_with("List row 100\n"))
    }));
    let parsed = cx.update(|_, cx| root.read(cx).card_docs.0.borrow().len());
    assert!(parsed < 50, "built {parsed} documents after jumping");
    for step in 1..=5 {
        cx.simulate_event(gpui::ScrollWheelEvent {
            position: point(px(350.), px(180.)),
            delta: gpui::ScrollDelta::Pixels(point(px(0.), px(-36.))),
            modifiers: Default::default(),
            touch_phase: gpui::TouchPhase::Moved,
        });
        settle(&mut cx);
        assert_eq!(scroll.offset().y, px(-3600. - 36. * step as f32));
        assert_eq!(scroll.max_offset(), max);
    }
    cx.update(|window, cx| {
        root.update(cx, |root, cx| {
            root.edit(None, Editing::Card(cards[0].clone()), window, cx);
        })
    });
    settle(&mut cx);
    cx.simulate_input("Editing still works");
    assert!(cx.update(|_, cx| {
        root.read(cx)
            .leaf()
            .card_field
            .read(cx)
            .content()
            .contains("Editing still works")
    }));
}

#[gpui::test]
fn list_window_tracks_group_folding_and_search(cx: &mut TestAppContext) {
    let (_scratch, root, member, _, mut cx) = open("list-groups", cx);
    cx.update(|window, cx| {
        root.update(cx, |root, cx| {
            root.close_card_preview(None, window, cx);
            root.workspace.update(cx, |workspace, _| {
                let board = &mut workspace.projects[0].boards[0];
                board.view = View::List;
                let first = board.columns[0].id.clone();
                let second = board.add_column("Second").id.clone();
                let hidden = board.add_column("Hidden").id.clone();
                for i in 0..60 {
                    board.add_card(&first, format!("First row {i}"));
                    board.add_card(&second, format!("Second row {i}"));
                    board.add_card(&hidden, format!("Hidden row {i}"));
                }
                board.columns[2].collapsed = true;
            });
            cx.notify();
        })
    });
    settle(&mut cx);
    let scroll = cx.update(|_, cx| root.read(cx).boards.of(&member.id).down.clone());
    let max = scroll.max_offset().y;
    assert!(!cx.update(|_, cx| {
        root.read(cx)
            .card_docs
            .0
            .borrow()
            .contains_key("Second row 0")
    }));
    cx.update(|_, cx| {
        root.update(cx, |root, cx| {
            root.workspace.update(cx, |workspace, _| {
                workspace.projects[0].boards[0].columns[0].collapsed = true
            });
            cx.notify();
        })
    });
    settle(&mut cx);
    assert_eq!(scroll.max_offset().y, max - px(62. * LIST_ROW_HEIGHT));
    assert!(cx.update(|_, cx| {
        root.read(cx)
            .card_docs
            .0
            .borrow()
            .contains_key("Second row 0")
    }));
    cx.update(|_, cx| {
        root.update(cx, |root, cx| {
            root.leaf_mut().finding = true;
            root.leaf()
                .find_field
                .update(cx, |field, cx| field.set_content("Hidden row 42", cx));
            cx.notify();
        })
    });
    settle(&mut cx);
    assert_eq!(scroll.max_offset().y, px(0.));
    assert!(cx.update(|_, cx| {
        root.read(cx)
            .card_docs
            .0
            .borrow()
            .contains_key("Hidden row 42")
    }));
}

#[gpui::test]
fn list_rows_open_drawer_and_group_controls_stay_independent(cx: &mut TestAppContext) {
    let (scratch, root, member, cards, mut cx) = open("list-controls", cx);
    // On disk as well: a save reads the board back.
    let store = fs::Project::new(&scratch.0);
    let mut board = store.board(&member.id).unwrap();
    board.view = View::List;
    store.save_board(&mut board).unwrap();
    cx.update(|window, cx| {
        root.update(cx, |root, cx| {
            root.close_card_preview(None, window, cx);
            root.workspace.update(cx, |workspace, _| {
                workspace.projects[0].boards[0].view = View::List
            });
            cx.notify();
        })
    });
    settle(&mut cx);
    let first = point(px(180.), px(LIST_HEADING_HEIGHT + LIST_ROW_HEIGHT / 2.));
    cx.simulate_mouse_down(first, MouseButton::Left, Modifiers::default());
    cx.simulate_mouse_up(first, MouseButton::Left, Modifiers::default());
    settle(&mut cx);
    assert!(cx.debug_bounds("card-drawer").is_some());
    assert!(cx.update(|_, cx| root.read(cx).leaf().editing.is_none()));
    assert_eq!(
        cx.update(|_, cx| root
            .read(cx)
            .leaf()
            .open_card
            .as_ref()
            .unwrap()
            .card
            .clone()),
        cards[0]
    );
    type_card(&root, "List text", &mut cx);
    click("card-drawer-close", &mut cx);
    assert_eq!(
        cx.update(
            |_, cx| root.read(cx).workspace.read(cx).projects[0].boards[0]
                .card(&cards[0])
                .unwrap()
                .text
                .trim_end()
                .to_owned()
        ),
        "List text"
    );
    click("list-group-toggle", &mut cx);
    assert!(cx.update(|_, cx| {
        root.read(cx).workspace.read(cx).projects[0].boards[0].columns[0].collapsed
    }));
    assert!(cx.update(|_, cx| root.read(cx).renaming.is_none()));
    click("list-group-add", &mut cx);
    assert!(cx.update(|_, cx| matches!(
        root.read(cx).leaf().editing,
        Some(Editing::New(Place::Top, _))
    )));
}

#[gpui::test]
fn list_drawer_reveals_lower_rows_and_restores_scroll(cx: &mut TestAppContext) {
    let (_scratch, root, member, _, mut cx) = open("list-reveal", cx);
    cx.update(|window, cx| {
        root.update(cx, |root, cx| {
            root.close_card_preview(None, window, cx);
            root.workspace.update(cx, |workspace, _| {
                let board = &mut workspace.projects[0].boards[0];
                board.view = View::List;
                let column = board.columns[0].id.clone();
                for i in 0..30 {
                    board.add_card(&column, format!("Task {i}"));
                }
            });
            cx.notify();
        })
    });
    settle(&mut cx);
    let scroll = cx.update(|_, cx| root.read(cx).boards.of(&member.id).down.clone());
    let before = scroll.offset();
    let position = point(px(180.), px(530.));
    cx.simulate_mouse_down(position, MouseButton::Left, Modifiers::default());
    cx.simulate_mouse_up(position, MouseButton::Left, Modifiers::default());
    settle(&mut cx);
    assert!(cx.debug_bounds("card-drawer").is_some());
    assert!(
        scroll.offset().y < before.y,
        "selected row should be revealed above the drawer"
    );
    click("card-drawer-close", &mut cx);
    assert_eq!(scroll.offset(), before);
}

#[gpui::test]
fn busy_orb_opens_its_session_without_opening_the_card(cx: &mut TestAppContext) {
    let (_scratch, root, _, cards, mut cx) = open("busy-orb", cx);
    cx.update(|window, cx| {
        root.update(cx, |root, cx| {
            root.close_card_preview(None, window, cx);
            root.workspace.update(cx, |workspace, _| {
                workspace.settings.features.sessions = true;
                let project = &mut workspace.projects[0];
                let chat = ChatSession::restore(
                    77,
                    project.path.clone(),
                    crate::model::settings::Agent {
                        name: "test".into(),
                        id: None,
                        command: String::new(),
                        args: Vec::new(),
                        env: Default::default(),
                    },
                    serde_json::from_value(serde_json::json!({
                        "id": "orb-session", "agent": "test", "title": "Polish boards",
                        "name": null, "updated": 1, "items": []
                    }))
                    .unwrap(),
                );
                project.sessions.push(chat);
                project.boards[0].view = View::List;
                project.boards[0].dispatch_card(&cards[0], "orb-session".into());
                project.boards[0].set_card_status(&cards[0], None);
            });
            // Linked and not busy: the session's number, not the orb.
            let card = &root.workspace.read(cx).projects[0].boards[0].columns[0].cards[0];
            let idle = root
                .card_working(card, root.card_session(card, cx))
                .unwrap();
            assert!(!idle.busy);
            assert_eq!(idle.session, Some((77, "Polish boards".into())));
            root.workspace.update(cx, |workspace, _| {
                workspace.projects[0].boards[0].set_card_status(&cards[0], Some(Status::Busy));
            });
            let card = &root.workspace.read(cx).projects[0].boards[0].columns[0].cards[0];
            let working = root
                .card_working(card, root.card_session(card, cx))
                .unwrap();
            assert!(working.busy);
            assert_eq!(working.session, Some((77, "Polish boards".into())));
            cx.notify();
        });
    });
    settle(&mut cx);
    click("card-busy-orb", &mut cx);
    cx.update(|_, cx| {
        let root = root.read(cx);
        assert_eq!(root.workspace.read(cx).projects[0].active, Some(77));
        assert!(matches!(root.leaf().pane, Pane::Chat));
        assert!(root.leaf().open_card.is_none());
    });
}

#[gpui::test]
fn list_cards_drop_into_a_collapsed_group_without_unfolding(cx: &mut TestAppContext) {
    for populated in [false, true] {
        let name = if populated {
            "collapsed-drop-full"
        } else {
            "collapsed-drop-empty"
        };
        let (_scratch, root, _, cards, mut cx) = open(name, cx);
        let target = cx.update(|window, cx| {
            root.update(cx, |root, cx| {
                root.close_card_preview(None, window, cx);
                let target = root.workspace.update(cx, |workspace, _| {
                    let project = &mut workspace.projects[0];
                    let store = project.store();
                    let board = &mut project.boards[0];
                    board.view = View::List;
                    let target = board.add_column("Later").id.clone();
                    if populated {
                        board.add_card(&target, "Already here".into());
                    }
                    board.columns[1].collapsed = true;
                    store.save_board(board).unwrap();
                    target
                });
                cx.notify();
                target
            })
        });
        settle(&mut cx);
        let source = point(px(180.), px(LIST_HEADING_HEIGHT + LIST_ROW_HEIGHT / 2.));
        let destination = point(
            px(180.),
            px(LIST_HEADING_HEIGHT * 1.5 + LIST_ROW_HEIGHT * 2.),
        );
        cx.simulate_mouse_down(source, MouseButton::Left, Modifiers::default());
        cx.simulate_mouse_move(
            source + point(px(15.), px(0.)),
            Some(MouseButton::Left),
            Modifiers::default(),
        );
        settle(&mut cx);
        cx.simulate_mouse_move(destination, Some(MouseButton::Left), Modifiers::default());
        settle(&mut cx);
        assert!(cx.update(|_, cx| root.read(cx).aimed_at(&target, None, cx)));
        let indicator = cx
            .debug_bounds("list-collapsed-drop-target")
            .expect("visible drop target");
        assert_eq!(indicator.size.height, px(LIST_HEADING_HEIGHT));
        cx.simulate_mouse_up(destination, MouseButton::Left, Modifiers::default());
        settle(&mut cx);
        cx.update(|_, cx| {
            let workspace = root.read(cx).workspace.read(cx);
            let column = workspace.projects[0].boards[0].column(&target).unwrap();
            assert!(column.collapsed);
            assert_eq!(column.cards.len(), if populated { 2 } else { 1 });
            assert_eq!(column.cards.last().unwrap().id, cards[0]);
        });
    }
}
