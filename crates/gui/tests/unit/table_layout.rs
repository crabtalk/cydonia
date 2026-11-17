use crate::{
    data::{ColType, Column, Data, Home},
    model::{project::Project, settings::Settings, state::State, workspace::Workspace},
    view::table::{self, Cell, Sizing, TablePreview, TableView},
};
use bezel::{
    gpui::{
        self, AppContext as _, Entity, Modifiers, MouseButton, ScrollDelta, ScrollEvent,
        ScrollSource, TestAppContext, VisualTestContext, WeakEntity, point, px, size,
    },
    theme::{Appearance, Theme},
    ui::input,
};
use std::path::PathBuf;

struct Directory(PathBuf);
impl Drop for Directory {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn open(
    cx: &mut TestAppContext,
    columns: usize,
    width: f32,
) -> (Directory, Entity<TableView>, VisualTestContext) {
    let path = std::env::temp_dir().join(format!("cydonia-table-layout-{}", uuid::Uuid::new_v4()));
    let mut data = Data::open(&Home::Project(path.clone())).unwrap();
    let columns: Vec<_> = (0..columns)
        .map(|ix| Column {
            name: format!("Field {ix}"),
            kind: ColType::Text,
        })
        .collect();
    let key = data.create("Stories", None, &columns, None).unwrap().key;
    data.add_rows(&key, 20).unwrap();
    cx.update(|cx| {
        Theme::install(Appearance::Dark, cx);
        input::init(cx);
    });
    let mut settings = Settings::default();
    settings.features.tables = true;
    let workspace = cx.new(|cx| Workspace::new(settings, State::default(), cx));
    let (entry, source) = workspace.update(cx, |workspace, cx| {
        let project = Project::new(workspace.intern(&path), path.clone(), cx);
        workspace.projects.push(project);
        let entry = workspace.table_member(0, 0).unwrap().into_entry().unwrap();
        let source = workspace.table_entity(&entry).unwrap().clone();
        (entry, source)
    });
    let window = cx.add_window(|_, cx| {
        TableView::new(entry, workspace, WeakEntity::new_invalid(), source, cx)
    });
    let view = window.root(cx).unwrap();
    let mut visual = VisualTestContext::from_window(window.into(), cx);
    visual.simulate_resize(size(px(width), px(400.)));
    settle(&mut visual);
    (Directory(path), view, visual)
}

fn settle(cx: &mut VisualTestContext) {
    for _ in 0..3 {
        cx.update(|window, _| window.refresh());
        cx.run_until_parked();
    }
}

#[gpui::test]
fn narrow_tables_keep_readable_columns_and_scroll_to_the_last_one(cx: &mut TestAppContext) {
    let (_directory, view, mut cx) = open(cx, 11, 600.);
    assert!(cx.debug_bounds("table-heading-0").unwrap().size.width >= px(table::COLUMN_WIDTH));
    let first = cx.debug_bounds("table-cell-1-Field 0").unwrap();
    let second = cx.debug_bounds("table-cell-1-Field 1").unwrap();
    assert_eq!(second.left() - first.left(), px(table::COLUMN_WIDTH));
    let (horizontal, vertical) = view.read_with(&cx, |view, _| {
        (view.horizontal.clone(), view.vertical.clone())
    });
    assert!(horizontal.max_offset().x > px(0.));
    assert!(vertical.max_offset().y > px(0.));
    let viewport = cx.debug_bounds("table-body").unwrap();
    cx.simulate_event(ScrollEvent {
        position: viewport.center(),
        delta: ScrollDelta::Pixels(point(px(-5000.), px(0.))),
        modifiers: Modifiers::default(),
        touch_phase: gpui::TouchPhase::Moved,
        source: ScrollSource::Pointer,
    });
    settle(&mut cx);
    assert!(horizontal.offset().x < px(0.));
    let last = cx.debug_bounds("table-heading-10").unwrap();
    assert!(last.right() <= viewport.right());
    assert!(last.left() >= viewport.left());
    let cell = cx.debug_bounds("table-cell-5-Field 10").unwrap();
    cx.simulate_event(ScrollEvent {
        position: viewport.center(),
        delta: ScrollDelta::Pixels(point(px(0.), px(-80.))),
        modifiers: Modifiers::default(),
        touch_phase: gpui::TouchPhase::Moved,
        source: ScrollSource::Pointer,
    });
    settle(&mut cx);
    assert!(vertical.offset().y < px(0.));
    assert_eq!(cx.debug_bounds("table-heading-10").unwrap(), last);
    let moved = cx.debug_bounds("table-cell-5-Field 10").unwrap();
    assert_eq!(moved.left(), cell.left());
    assert!(moved.top() < cell.top());
}

#[gpui::test]
fn dragging_a_header_resizes_its_cells_without_starting_an_edit(cx: &mut TestAppContext) {
    let (_directory, view, mut cx) = open(cx, 3, 700.);
    let width = cx.debug_bounds("table-heading-0").unwrap().size.width;
    let next = cx.debug_bounds("table-heading-1").unwrap().size.width;
    let grip = cx.debug_bounds("table-resize-0").unwrap().center();
    cx.simulate_mouse_down(grip, MouseButton::Left, Modifiers::default());
    assert!(
        view.read_with(&cx, |view, _| view.resizing.is_some()),
        "the grip takes the press"
    );
    cx.simulate_mouse_move(
        grip + point(px(10.), px(0.)),
        Some(MouseButton::Left),
        Modifiers::default(),
    );
    assert!(
        cx.update(|_, cx| cx.has_active_drag()),
        "the grip starts a column drag"
    );
    cx.simulate_mouse_move(
        grip + point(px(120.), px(0.)),
        Some(MouseButton::Left),
        Modifiers::default(),
    );
    cx.simulate_mouse_up(
        grip + point(px(120.), px(0.)),
        MouseButton::Left,
        Modifiers::default(),
    );
    settle(&mut cx);
    assert_eq!(
        cx.debug_bounds("table-heading-0").unwrap().size.width,
        width + px(120.)
    );
    assert_eq!(cx.debug_bounds("table-heading-1").unwrap().size.width, next);
    let first = cx.debug_bounds("table-cell-1-Field 0").unwrap();
    let second = cx.debug_bounds("table-cell-1-Field 1").unwrap();
    assert_eq!(second.left() - first.left(), width + px(120.));
    assert!(view.read_with(&cx, |view, _| view.cell.is_none()));
    let grip = cx.debug_bounds("table-resize-0").unwrap().center();
    let end = point(px(60.), grip.y);
    cx.simulate_mouse_down(grip, MouseButton::Left, Modifiers::default());
    cx.simulate_mouse_move(
        grip - point(px(10.), px(0.)),
        Some(MouseButton::Left),
        Modifiers::default(),
    );
    cx.simulate_mouse_move(end, Some(MouseButton::Left), Modifiers::default());
    cx.simulate_mouse_up(end, MouseButton::Left, Modifiers::default());
    settle(&mut cx);
    assert_eq!(
        cx.debug_bounds("table-heading-0").unwrap().size.width,
        px(table::MIN_COLUMN_WIDTH)
    );
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.cell = Some(Cell::Head("Field 0".into()));
            view.cell_field
                .update(cx, |field, cx| field.set_content("Renamed", cx));
            assert!(view.commit_cell(cx));
        })
    });
    cx.simulate_resize(size(px(500.), px(400.)));
    settle(&mut cx);
    assert_eq!(
        cx.debug_bounds("table-heading-0").unwrap().size.width,
        px(table::MIN_COLUMN_WIDTH)
    );
}

#[gpui::test]
fn dragging_a_column_persists_order_and_keeps_its_cells_together(cx: &mut TestAppContext) {
    let (directory, _, mut cx) = open(cx, 3, 700.);
    let first = cx.debug_bounds("table-heading-0").unwrap().center();
    let last = cx.debug_bounds("table-heading-2").unwrap();
    let end = point(last.right() - px(20.), first.y);
    cx.simulate_mouse_down(first, MouseButton::Left, Modifiers::default());
    cx.simulate_mouse_move(
        first + point(px(12.), px(0.)),
        Some(MouseButton::Left),
        Modifiers::default(),
    );
    assert!(cx.update(|_, cx| cx.has_active_drag()));
    settle(&mut cx);
    cx.simulate_mouse_move(end, Some(MouseButton::Left), Modifiers::default());
    settle(&mut cx);
    cx.simulate_mouse_up(end, MouseButton::Left, Modifiers::default());
    settle(&mut cx);
    let data = Data::open(&Home::Project(directory.0.clone())).unwrap();
    let table = data.table("stories").unwrap();
    assert_eq!(table.column_layout.order(&table.columns), [1, 2, 0]);
    let moved = cx.debug_bounds("table-cell-1-Field 0").unwrap();
    let left = cx.debug_bounds("table-cell-1-Field 2").unwrap();
    assert!(moved.left() > left.left());
}

#[gpui::test]
fn table_sort_runs_before_paging_and_can_be_cleared(cx: &mut TestAppContext) {
    let (directory, view, mut cx) = open(cx, 1, 650.);
    let mut data = Data::open(&Home::Project(directory.0.clone())).unwrap();
    data.add_rows("stories", 182).unwrap();
    data.write(r#"UPDATE stories SET "Field 0" = 'a' WHERE rowid IN (201, 202); UPDATE stories SET "Field 0" = 'z' WHERE rowid = 1"#)
        .unwrap();
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.source.update(cx, |source, cx| {
                source.adopt(data.table("stories").unwrap(), cx);
            });
            view.show_page(200, cx);
            let sort = |descending| {
                Some(table::model::Sort {
                    column: "Field 0".into(),
                    descending,
                })
            };
            view.sort_rows(sort(false), cx);
            assert_eq!(view.offset, 0);
            let ids: Vec<_> = view
                .page(cx)
                .unwrap()
                .rows
                .iter()
                .take(3)
                .map(|row| row.rowid)
                .collect();
            assert_eq!(ids, [201, 202, 1]);
            view.sort_rows(sort(true), cx);
            assert_eq!(view.page(cx).unwrap().rows[0].rowid, 1);
            view.show_page(200, cx);
            assert_eq!(view.page(cx).unwrap().rows[0].rowid, 199);
            view.sort_rows(None, cx);
            assert_eq!(view.page(cx).unwrap().rows[1].rowid, 2);
        });
    });
}

#[gpui::test]
fn table_previews_scroll_instead_of_compressing_columns(cx: &mut TestAppContext) {
    let (_directory, view, mut visual) = open(cx, 11, 600.);
    let source = view.read_with(&visual, |view, _| view.source.clone());
    let preview = visual.add_window(|_, cx| {
        cx.observe(&source, |_, _, cx| cx.notify()).detach();
        TablePreview {
            source,
            sizing: Sizing::default(),
            scroll: gpui::ScrollHandle::new(),
        }
    });
    let view = preview.root(&mut visual).unwrap();
    let mut visual = VisualTestContext::from_window(preview.into(), &visual);
    visual.simulate_resize(size(px(500.), px(300.)));
    settle(&mut visual);
    view.read_with(&visual, |view, _| {
        assert!(view.scroll.max_offset().x > px(1400.));
        assert!(view.scroll.max_offset().y > px(0.));
    });
}

#[gpui::test]
fn a_reloaded_table_does_not_apply_a_draft_to_a_replacement_row(cx: &mut TestAppContext) {
    let (directory, view, mut cx) = open(cx, 1, 600.);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.start_edit(
                Cell::Value {
                    rowid: 1,
                    column: "Field 0".into(),
                },
                window,
                cx,
            );
            view.cell_field
                .update(cx, |field, cx| field.set_content("Draft", cx));
        })
    });
    let mut data = Data::open(&Home::Project(directory.0.clone())).unwrap();
    data.write(r#"DELETE FROM stories; INSERT INTO stories("Field 0") VALUES('Replacement')"#)
        .unwrap();
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.source.update(cx, |source, cx| {
                source.adopt(data.table("stories").unwrap(), cx)
            });
            assert!(!view.commit_cell(cx));
            assert!(view.cell.is_some());
            assert_eq!(view.cell_field.read(cx).content(), "Draft");
            assert!(view.source.read(cx).error.is_some());
        })
    });
    assert_eq!(
        data.read("stories", None, false, 10, 0).unwrap().rows[0].cells[0],
        "Replacement"
    );
}

#[gpui::test]
fn table_paging_reaches_later_rows_and_opens_new_rows_on_their_page(cx: &mut TestAppContext) {
    let (directory, view, mut cx) = open(cx, 2, 800.);
    let mut data = Data::open(&Home::Project(directory.0.clone())).unwrap();
    data.add_rows("stories", 405).unwrap();
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.source.update(cx, |source, cx| {
                source.adopt(data.table("stories").unwrap(), cx)
            });
        })
    });
    settle(&mut cx);
    let next = cx.debug_bounds("table-next").unwrap().center();
    cx.simulate_mouse_down(next, MouseButton::Left, Modifiers::default());
    cx.simulate_mouse_up(next, MouseButton::Left, Modifiers::default());
    settle(&mut cx);
    view.read_with(&cx, |view, cx| {
        assert_eq!(view.offset, 200);
        let page = view.page(cx).unwrap();
        assert_eq!(page.rows[0].rowid, 201);
        assert_eq!(page.rows.len(), 200);
        assert_eq!(view.source.read(cx).page(0).unwrap().rows[0].rowid, 1);
    });
    cx.update(|window, cx| view.update(cx, |view, cx| view.add_row(window, cx)));
    settle(&mut cx);
    view.read_with(&cx, |view, cx| {
        assert_eq!(view.offset, 400);
        assert_eq!(view.page(cx).unwrap().total, 426);
        assert!(
            view.page(cx)
                .unwrap()
                .rows
                .iter()
                .any(|row| row.rowid == 426)
        );
        assert_eq!(
            view.cell,
            Some(Cell::Value {
                rowid: 426,
                column: "Field 0".into()
            })
        );
    });
    let source = view.read_with(&cx, |view, _| view.source.clone());
    let second = cx.add_window(|_, cx| {
        let original = view.read(cx);
        TableView::new(
            original.table.clone(),
            original.workspace.clone(),
            WeakEntity::new_invalid(),
            source,
            cx,
        )
    });
    let second = second.root(&mut cx).unwrap();
    assert_eq!(second.read_with(&cx, |view, _| view.offset), 0);
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.drop_cell(cx);
            view.show_page(200, cx);
            assert_eq!(view.offset, 200);
        })
    });
    assert_eq!(second.read_with(&cx, |view, _| view.offset), 0);
    data.delete_rows("stories", &(201..=426).collect::<Vec<_>>())
        .unwrap();
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.source.update(cx, |source, cx| {
                source.adopt(data.table("stories").unwrap(), cx)
            });
        })
    });
    settle(&mut cx);
    view.read_with(&cx, |view, cx| {
        assert_eq!(view.offset, 0);
        assert_eq!(view.page(cx).unwrap().rows.len(), 200);
    });
}

#[gpui::test]
fn unchanged_cells_preserve_spaces_empty_strings_nulls_and_storage_types(cx: &mut TestAppContext) {
    let (directory, view, mut cx) = open(cx, 1, 600.);
    let mut data = Data::open(&Home::Project(directory.0.clone())).unwrap();
    data.write_column("stories", "Field 0", Some(ColType::Number), None)
        .unwrap();
    let connection = rusqlite::Connection::open(directory.0.join(".cydonia/data.db")).unwrap();
    connection.execute_batch(r#"DELETE FROM stories; INSERT INTO stories("Field 0") VALUES('  keep  '), (''), (NULL), (44); CREATE TRIGGER reject_unchanged BEFORE UPDATE ON stories BEGIN SELECT RAISE(ABORT, 'unchanged cells must not write'); END;"#).unwrap();
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.source.update(cx, |source, cx| {
                source.adopt(data.table("stories").unwrap(), cx)
            });
        })
    });
    for id in 1..=4 {
        cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                view.start_edit(
                    Cell::Value {
                        rowid: id,
                        column: "Field 0".into(),
                    },
                    window,
                    cx,
                );
                assert!(view.commit_cell(cx));
                assert!(view.cell.is_none());
            })
        });
    }
    let rows = data.read("stories", None, false, 10, 0).unwrap().rows;
    assert_eq!(rows[0].cells[0], "  keep  ");
    assert_eq!(rows[1].cells[0], "");
    assert!(rows[2].cells[0].is_null());
    assert_eq!(rows[3].cells[0], 44);
    connection
        .execute_batch("DROP TRIGGER reject_unchanged")
        .unwrap();
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.source.update(cx, |source, cx| {
                source.adopt(data.table("stories").unwrap(), cx)
            });
        })
    });
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.start_edit(
                Cell::Value {
                    rowid: 1,
                    column: "Field 0".into(),
                },
                window,
                cx,
            );
            view.cell_field
                .update(cx, |field, cx| field.set_content("  changed  ", cx));
            assert!(view.commit_cell(cx));
        })
    });
    assert_eq!(
        data.read("stories", None, false, 10, 0).unwrap().rows[0].cells[0],
        "  changed  "
    );
}
