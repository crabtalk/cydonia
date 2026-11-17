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
    let scroll = view.read_with(&cx, |view, _| view.scroll.clone());
    assert!(scroll.max_offset().x > px(0.));
    assert!(scroll.max_offset().y > px(0.));
    let viewport = cx.debug_bounds("table-body").unwrap();
    cx.simulate_event(ScrollEvent {
        position: viewport.center(),
        delta: ScrollDelta::Pixels(point(px(-5000.), px(0.))),
        modifiers: Modifiers::default(),
        touch_phase: gpui::TouchPhase::Moved,
        source: ScrollSource::Pointer,
    });
    settle(&mut cx);
    assert!(scroll.offset().x < px(0.));
    let last = cx.debug_bounds("table-heading-10").unwrap();
    assert!(last.right() <= viewport.right());
    assert!(last.left() >= viewport.left());
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
