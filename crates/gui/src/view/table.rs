//! The table pane: one table's rows, and the sidebar row that opens it.
//!
use crate::{
    data::{ColType, Column, ColumnLayout, Revision},
    model::{
        table::{self as model, Table},
        workspace::{Entry, Member, Opening, Workspace},
    },
    view::{
        kit::{
            add, link,
            menu::{self, MenuHost},
        },
        window::root::{Cydonia, NewTable},
    },
};
use bezel::{
    gpui::{
        self, AnyElement, App, Axis, Context, Div, DragMoveEvent, Empty, Entity, EntityId,
        Focusable as _, KeyBinding, MouseButton, Pixels, ScrollHandle, SharedString, Stateful,
        WeakEntity, Window, actions, div, prelude::*, px,
    },
    theme::{Selection, TextStyle, Theme, Typeset},
    ui::{
        context_menu::{ContextMenu, Place},
        drag, icons,
        input::{Shape, TextField},
        menu::Item,
        scroll as scrollbars, table,
        tooltip::Tooltip,
        widgets::{Buttons, Layout as _, SplitStyle},
    },
};
use markdown::{Link, Links as _};
use serde_json::Value;
use std::{cell::Cell as ValueCell, rc::Rc};

/// Claimed on top of `TextField`, so `enter` files the cell here and stays a
/// newline in every other field.
const KEY_CONTEXT: &str = "CydoniaCell";

/// The line box every cell keeps, focused or not. It is what `TextField`
/// renders at and cannot be told otherwise, so a cell that sized itself to its
/// own text would grow the row the moment you clicked into it.
const LINE: f32 = 18.;

/// The column the row actions sit in — the trash on a row, the `+` on the
/// header. Declared with the data columns so both halves line up.
const ACTIONS: f32 = 62.;

const COLUMN_WIDTH: f32 = 180.;
const MIN_COLUMN_WIDTH: f32 = 80.;
const COLUMN_RESIZE_HIT: f32 = 12.;
const INSET: f32 = 24.;
const PREVIEW_INSET: f32 = 16.;

#[derive(Default)]
struct Sizing {
    available: Rc<ValueCell<Pixels>>,
}

impl Sizing {
    fn columns(
        &self,
        columns: &[Column],
        order: &[usize],
        layout: &ColumnLayout,
        resizing: Option<&Resizing>,
        tail: Pixels,
    ) -> (Vec<table::Column>, Pixels) {
        let default = ((self.available.get() - tail - px(2.)) / columns.len().max(1) as f32)
            .max(px(COLUMN_WIDTH));
        let mut width = tail + px(2.);
        let declared = order
            .iter()
            .map(|&ix| {
                let column = &columns[ix];
                let measure = resizing
                    .filter(|resize| resize.column == column.name)
                    .map(|resize| resize.current)
                    .or_else(|| {
                        layout
                            .settings
                            .get(&column.name)?
                            .width
                            .map(|width| px(width as f32).max(px(MIN_COLUMN_WIDTH)))
                    })
                    .unwrap_or(default);
                width += measure;
                let declared =
                    table::Column::new(column.name.clone(), table::Width::Fixed(measure));
                match column.kind {
                    ColType::Number => declared.align_end(),
                    _ => declared,
                }
            })
            .collect();
        (declared, width)
    }

    fn measure<V: 'static>(&self, inset: f32, cx: &Context<V>) -> AnyElement {
        let available = self.available.clone();
        let view = cx.weak_entity();
        gpui::canvas(
            move |bounds, _, cx| {
                let width = (bounds.size.width - px(inset * 2.)).max(px(0.));
                if available.replace(width) != width {
                    view.update(cx, |_, cx| cx.notify()).ok();
                }
            },
            |_, _, _, _| {},
        )
        .absolute()
        .size_full()
        .into_any_element()
    }
}

struct Resizing {
    column: String,
    start: Pixels,
    width: Pixels,
    current: Pixels,
}

struct ColumnResize(EntityId);

#[derive(Clone)]
struct RowDrag {
    id: i64,
    revision: Revision,
}

impl PartialEq for RowDrag {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

actions!(cydonia_table, [CommitCell, DismissCell]);

pub fn bindings() -> Vec<KeyBinding> {
    let ctx = Some(KEY_CONTEXT);
    vec![
        KeyBinding::new("enter", CommitCell, ctx),
        KeyBinding::new("escape", DismissCell, ctx),
    ]
}

/// The pane's field for cells and column headings.
pub fn field(cx: &mut App) -> Entity<TextField> {
    cx.new(|cx| {
        TextField::new(cx)
            .with_shape(Shape::Line)
            .with_key_context(KEY_CONTEXT)
            .with_frame(false)
    })
}

/// What the field is attached to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Cell {
    /// A value being written, in the row with this id.
    Value { rowid: i64, column: String },
    /// A column being renamed.
    Head(String),
    Option {
        column: String,
        previous: Option<String>,
    },
}

#[derive(Clone, PartialEq, Eq)]
pub(crate) enum TableMenu {
    Column(String),
    Select { rowid: i64, column: String },
}

impl Cydonia {
    // ── mutations ────────────────────────────────────────────────

    /// The menu's New Table. The sidebar's `+` names a project by the heading
    /// it sits under; the menu bar has only the one in front.
    pub(crate) fn new_table_action(
        &mut self,
        _: &NewTable,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(project) = self.workspace.read(cx).active else {
            return;
        };
        self.new_table(project, window, cx);
    }

    pub(crate) fn new_table(
        &mut self,
        project: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.commit(cx) {
            return;
        }
        self.select_project(project, cx);
        let ix = self
            .workspace
            .update(cx, |workspace, cx| workspace.new_table(cx));
        if let Some(ix) = ix
            && let Some(member) = self.workspace.read(cx).table_member(project, ix)
        {
            self.land_member(member, window, cx);
        }
    }

    pub(crate) fn open_table(
        &mut self,
        project: usize,
        ix: usize,
        opening: Opening,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.commit(cx) {
            return;
        }
        let Some(member) = self.workspace.read(cx).table_member(project, ix) else {
            return;
        };
        self.enter_member(member, opening, window, cx);
        cx.notify();
    }

    /// The table view `on`'s pane draws, where it shows a table.
    pub(crate) fn table_view_of(&self, on: Option<&Member>, cx: &App) -> Option<Entity<TableView>> {
        self.leaf_of(on, cx)?.read(cx).table_shown(cx)
    }

    /// Every table view the panes hold.
    pub(crate) fn table_views(&self, cx: &App) -> Vec<Entity<TableView>> {
        self.leaves
            .iter()
            .flat_map(|pane| pane.read(cx).tables.clone())
            .collect()
    }

    /// File every table view's open field. Called from [`Cydonia::commit`],
    /// so every way out of a pane goes through it.
    pub(crate) fn commit_cell(&mut self, cx: &mut Context<Self>) -> bool {
        self.table_views(cx)
            .into_iter()
            .all(|view| view.update(cx, |view, cx| view.commit_cell(cx)))
    }

    // ── chrome ───────────────────────────────────────────────────

    pub(crate) fn table_peek(
        &self,
        project: usize,
        at: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let source = self
            .workspace
            .read(cx)
            .projects
            .get(project)?
            .tables
            .get(at)?
            .table
            .clone();
        let view = window.use_keyed_state(
            SharedString::from(format!("table-peek-{:?}", source.entity_id())),
            cx,
            move |_, cx| {
                cx.observe(&source, |_, _, cx| cx.notify()).detach();
                source.update(cx, |table, cx| table.load(cx));
                TablePreview {
                    source,
                    sizing: Sizing::default(),
                    scroll: ScrollHandle::new(),
                }
            },
        );
        Some(view.into_any_element())
    }
}

struct TablePreview {
    source: Entity<Table>,
    sizing: Sizing,
    scroll: ScrollHandle,
}

impl Render for TablePreview {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = Theme::of(cx).clone();
        let source = self.source.read(cx);
        let Some(page) = source.page(0) else {
            return div()
                .text_color(theme.danger)
                .child(source.error.clone().unwrap_or_default())
                .into_any_element();
        };
        let layout = &source.record.column_layout;
        let order = layout.order(&page.columns);
        let (declared, width) = self
            .sizing
            .columns(&page.columns, &order, layout, None, px(0.));
        let headings = declared
            .iter()
            .map(|column| table::header_cell(&theme, column, None).cursor_default());
        let mut body = table::table(&theme)
            .w(width)
            .min_w_full()
            .child(table::header(&theme).children(headings));
        for (n, record) in page.rows.iter().enumerate() {
            let cells = order
                .iter()
                .map(|&ix| {
                    div()
                        .line_height(px(LINE))
                        .child(record.cells.get(ix).map(text).unwrap_or_default())
                        .into_any_element()
                })
                .collect();
            body = body.child(table::row(
                &theme,
                &declared,
                n == 0,
                Selection::None,
                cells,
            ));
        }
        let count = match page.total as usize == page.rows.len() {
            true => format!("{} rows", page.total),
            false => format!("{} of {} rows", page.rows.len(), page.total),
        };
        div()
            .relative()
            .flex_1()
            .min_w_0()
            .min_h_0()
            .flex()
            .flex_col()
            .child(
                scrollbars::pane("table-peek", scrollbars::Axes::Both)
                    .flex_1()
                    .min_w_0()
                    .min_h_0()
                    .track_scroll(&self.scroll)
                    .px(px(PREVIEW_INSET))
                    .py(px(12.))
                    .child(body)
                    .child(
                        div()
                            .mt(px(8.))
                            .text_style(TextStyle::Subheadline)
                            .text_color(theme.text_faint)
                            .child(count),
                    ),
            )
            .child(scrollbars::Overlay::new(
                "table-peek-vertical",
                &self.scroll,
                Axis::Vertical,
            ))
            .child(scrollbars::Overlay::new(
                "table-peek-horizontal",
                &self.scroll,
                Axis::Horizontal,
            ))
            .child(self.sizing.measure(PREVIEW_INSET, cx))
            .into_any_element()
    }
}

/// One pane's view of one table: its rows, with the one field that writes
/// them and the column menus it opens.
pub(crate) struct TableView {
    /// The table, as the space names it.
    pub(crate) table: Entry,
    workspace: Entity<Workspace>,
    source: Entity<Table>,
    sizing: Sizing,
    horizontal: ScrollHandle,
    vertical: ScrollHandle,
    offset: i64,
    resizing: Option<Resizing>,
    column_drag: drag::Domain<(), String>,
    row_drag: drag::Domain<(), RowDrag>,
    root: WeakEntity<Cydonia>,
    /// What the field is attached to, and the field itself.
    pub(crate) cell: Option<Cell>,
    revision: Option<Revision>,
    original: Option<String>,
    pub(crate) cell_field: Entity<TextField>,
    menu: ContextMenu<Self, TableMenu>,
}

impl MenuHost for TableView {
    type Key = TableMenu;

    fn menu(&self) -> &ContextMenu<Self, TableMenu> {
        &self.menu
    }

    fn menu_mut(&mut self) -> &mut ContextMenu<Self, TableMenu> {
        &mut self.menu
    }
}

impl TableView {
    pub(crate) fn new(
        table: Entry,
        workspace: Entity<Workspace>,
        root: WeakEntity<Cydonia>,
        source: Entity<Table>,
        cx: &mut Context<Self>,
    ) -> Self {
        cx.observe(&source, |_, _, cx| cx.notify()).detach();
        source.update(cx, |table, cx| table.load(cx));
        Self {
            table,
            workspace,
            source,
            sizing: Sizing::default(),
            horizontal: ScrollHandle::new(),
            vertical: ScrollHandle::new(),
            offset: 0,
            resizing: None,
            column_drag: drag::Domain::with_ghost(|name: &String, _, _| {
                drag::ghost_label("table-column-ghost", name.clone()).into_any_element()
            }),
            row_drag: drag::Domain::with_ghost(|_, _, _| {
                drag::ghost_label("table-row-ghost", "Move row").into_any_element()
            }),
            root,
            cell: None,
            revision: None,
            original: None,
            cell_field: field(cx),
            menu: ContextMenu::default(),
        }
    }

    fn page<'a>(&self, cx: &'a App) -> Option<&'a crate::data::Page> {
        self.source.read(cx).page(self.offset)
    }

    fn show_page(&mut self, offset: i64, cx: &mut Context<Self>) {
        if !self.commit_cell(cx) {
            return;
        }
        let offset = offset.max(0).min(self.source.read(cx).last_page());
        self.source
            .update(cx, |source, cx| source.load_page(offset, cx));
        if self.source.read(cx).page(offset).is_some() {
            self.offset = offset;
            self.vertical.set_offset(gpui::point(px(0.), px(0.)));
            cx.notify();
        }
    }

    fn sort_rows(&mut self, sort: Option<model::Sort>, cx: &mut Context<Self>) {
        if self.commit_cell(cx) {
            self.source.update(cx, |table, cx| table.set_sort(sort, cx));
            self.show_page(0, cx);
        }
    }

    /// Point the field at `at`, filing whatever any pane had open first — so
    /// clicking straight from one cell to another never drops an edit.
    fn edit_cell(&mut self, at: Cell, window: &mut Window, cx: &mut Context<Self>) {
        let (root, view) = (self.root.clone(), cx.entity());
        window.defer(cx, move |window, cx| {
            if root
                .update(cx, |root, cx| root.commit(cx))
                .is_ok_and(|saved| !saved)
            {
                return;
            }
            view.update(cx, |view, cx| {
                if view.cell.is_none() {
                    view.start_edit(at, window, cx);
                }
            });
        });
    }

    fn start_edit(&mut self, at: Cell, window: &mut Window, cx: &mut Context<Self>) {
        if let Cell::Value { rowid, column } = &at
            && self
                .source
                .read(cx)
                .record
                .columns
                .iter()
                .any(|held| held.name == *column && held.kind == ColType::Select)
        {
            self.open_select(*rowid, column, window, cx);
            return;
        }
        let text = match &at {
            Cell::Value { rowid, column } => self
                .page(cx)
                .and_then(|page| {
                    let ix = page.columns.iter().position(|held| held.name == *column)?;
                    page.rows
                        .iter()
                        .find(|record| record.rowid == *rowid)?
                        .cells
                        .get(ix)
                })
                .map(text)
                .unwrap_or_default(),
            Cell::Head(name) => name.clone(),
            Cell::Option { previous, .. } => previous.clone().unwrap_or_default(),
        };
        self.original = Some(text.clone());
        self.cell_field
            .update(cx, |field, cx| field.set_content(text, cx));
        self.revision = self.page(cx).map(|page| page.revision);
        self.cell = Some(at);
        window.focus(&self.cell_field.read(cx).focus_handle(cx), cx);
        cx.notify();
    }

    pub(crate) fn commit_cell(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(at) = self.cell.clone() else {
            return true;
        };
        let text = self.cell_field.read(cx).content().to_string();
        if self.original.as_deref() == Some(&text) {
            self.drop_cell(cx);
            return true;
        }
        let text = match at {
            Cell::Value { .. } => text,
            _ => text.trim().to_owned(),
        };
        let entry = self.table.clone();
        let revision = self.revision;
        let saved = self.workspace.update(cx, |workspace, cx| {
            workspace.edit_table(&entry, cx, |table, cx| match at {
                Cell::Value { rowid, column } => {
                    table.write_cell(rowid, column, text, revision, cx)
                }
                Cell::Head(column) if !text.is_empty() => {
                    table.write_column(&column, None, Some(&text), cx)
                }
                Cell::Option { column, previous } if !text.is_empty() => {
                    table.write_option(&column, previous.as_deref(), &text, cx)
                }
                _ => Ok(()),
            })
        });
        if saved.is_ok() {
            self.drop_cell(cx);
        }
        cx.notify();
        saved.is_ok()
    }

    /// Let go of the field without filing it.
    fn drop_cell(&mut self, cx: &mut Context<Self>) {
        self.cell = None;
        self.original = None;
        self.revision = None;
        self.cell_field.update(cx, |field, cx| field.clear(cx));
        cx.notify();
    }

    fn open_select(
        &mut self,
        rowid: i64,
        column: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(page) = self.page(cx) else { return };
        let Some(column_index) = page.columns.iter().position(|held| held.name == column) else {
            return;
        };
        let Some(row) = page.rows.iter().find(|held| held.rowid == rowid) else {
            return;
        };
        let revision = page.revision;
        let current = row.cells.get(column_index).and_then(Value::as_str);
        let options = self
            .source
            .read(cx)
            .record
            .column_layout
            .settings
            .get(column)
            .map(|display| display.select_options.as_slice())
            .unwrap_or_default();
        let mut rows = vec![menu::row::<Self>(
            Item::action("Clear selection").disabled(current.is_none()),
            {
                let column = column.to_owned();
                move |this, _, cx| this.choose_option(rowid, &column, "", revision, cx)
            },
        )];
        rows.extend(options.iter().map(|option| {
            let column = column.to_owned();
            let option = option.clone();
            menu::row::<Self>(
                Item::action(option.clone()).checked(current == Some(option.as_str())),
                move |this, _, cx| this.choose_option(rowid, &column, &option, revision, cx),
            )
        }));
        self.toggle_menu(
            TableMenu::Select {
                rowid,
                column: column.to_owned(),
            },
            Place::Below,
            move |_, _, _| rows,
            window,
            cx,
        );
    }

    fn choose_option(
        &mut self,
        rowid: i64,
        column: &str,
        option: &str,
        revision: Revision,
        cx: &mut Context<Self>,
    ) {
        let entry = self.table.clone();
        let _ = self.workspace.update(cx, |workspace, cx| {
            workspace.edit_table(&entry, cx, |table, cx| {
                table.write_cell(
                    rowid,
                    column.to_owned(),
                    option.to_owned(),
                    Some(revision),
                    cx,
                )
            })
        });
    }

    fn commit_cell_action(&mut self, _: &CommitCell, _: &mut Window, cx: &mut Context<Self>) {
        self.commit_cell(cx);
    }

    /// Drop the edit and leave what was there.
    fn dismiss_cell(&mut self, _: &DismissCell, _: &mut Window, cx: &mut Context<Self>) {
        self.drop_cell(cx);
    }

    fn add_row(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.commit_cell(cx) {
            return;
        }
        let entry = self.table.clone();
        let rowid = self.workspace.update(cx, |workspace, cx| {
            workspace.edit_table(&entry, cx, |table, cx| table.add_row(cx))
        });
        if let Ok(rowid) = rowid
            && let Some(column) = self
                .source
                .read(cx)
                .record
                .columns
                .first()
                .map(|column| column.name.clone())
        {
            self.show_page(self.source.read(cx).last_page(), cx);
            self.horizontal.set_offset(gpui::point(px(0.), px(0.)));
            self.vertical.scroll_to_bottom();
            self.edit_cell(
                Cell::Value {
                    rowid,
                    column: column.clone(),
                },
                window,
                cx,
            );
        }
    }

    fn delete_row(&mut self, rowid: i64, cx: &mut Context<Self>) {
        if !self.commit_cell(cx) {
            return;
        }
        let entry = self.table.clone();
        let _ = self.workspace.update(cx, |workspace, cx| {
            workspace.edit_table(&entry, cx, |table, cx| table.delete_row(rowid, cx))
        });
    }

    fn move_row(
        &mut self,
        event: &drag::Drop<(), RowDrag>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.source.read(cx).sort.is_none() && self.commit_cell(cx) {
            let entry = self.table.clone();
            let _ = self.workspace.update(cx, |workspace, cx| {
                workspace.edit_table(&entry, cx, |table, cx| {
                    table.move_row(
                        event.item.id,
                        event.after.as_ref().map(|row| row.id),
                        event.before.as_ref().map(|row| row.id),
                        event.item.revision,
                        cx,
                    )
                })
            });
        }
    }

    fn add_column(&mut self, cx: &mut Context<Self>) {
        if !self.commit_cell(cx) {
            return;
        }
        let entry = self.table.clone();
        let _ = self.workspace.update(cx, |workspace, cx| {
            workspace.edit_table(&entry, cx, |table, cx| table.add_column(cx))
        });
    }

    fn retype_column(&mut self, column: &str, kind: ColType, cx: &mut Context<Self>) {
        if !self.commit_cell(cx) {
            return;
        }
        let entry = self.table.clone();
        let _ = self.workspace.update(cx, |workspace, cx| {
            workspace.edit_table(&entry, cx, |table, cx| {
                table.write_column(column, Some(kind), None, cx)
            })
        });
    }

    fn delete_column(&mut self, column: &str, cx: &mut Context<Self>) {
        if !self.commit_cell(cx) {
            return;
        }
        let entry = self.table.clone();
        let _ = self.workspace.update(cx, |workspace, cx| {
            workspace.edit_table(&entry, cx, |table, cx| table.delete_column(column, cx))
        });
    }

    /// The field, wired so a click anywhere else files it.
    ///
    /// gpui holds focus until something takes it, and `TextField` carries no
    /// blur policy of its own — it cannot know whether leaving means commit or
    /// cancel. Here it means commit, the same as `enter`.
    fn cell_editor(&self, cx: &mut Context<Self>) -> Div {
        div()
            .w_full()
            .child(self.cell_field.clone())
            .on_mouse_down_out(cx.listener(|this, _, _, cx| {
                this.commit_cell(cx);
            }))
    }

    fn finish_resize(&mut self, cx: &mut Context<Self>) {
        let Some(resize) = self.resizing.take() else {
            return;
        };
        if resize.current != resize.width {
            let entry = self.table.clone();
            let width = f32::from(resize.current).round() as u32;
            let _ = self.workspace.update(cx, |workspace, cx| {
                workspace.edit_table(&entry, cx, |table, cx| {
                    table.set_column_width(&resize.column, width, cx)
                })
            });
        }
        cx.notify();
    }

    fn move_column(
        &mut self,
        event: &drag::Drop<(), String>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.commit_cell(cx) {
            let entry = self.table.clone();
            let _ = self.workspace.update(cx, |workspace, cx| {
                workspace.edit_table(&entry, cx, |table, cx| {
                    table.move_column(
                        &event.item,
                        event.after.as_deref(),
                        event.before.as_deref(),
                        cx,
                    )
                })
            });
        }
    }

    /// One heading: the column's name, and the menu that changes what it is.
    fn heading(
        &self,
        ix: usize,
        kind: ColType,
        shape: &table::Column,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let theme = Theme::of(cx).clone();
        let table::Width::Fixed(width) = shape.width else {
            unreachable!("table columns have measured widths");
        };
        let column = shape.label.to_string();
        let sorted = self
            .source
            .read(cx)
            .sort
            .as_ref()
            .filter(|sort| sort.column == column)
            .map(|sort| !sort.descending);
        let dragging = self
            .resizing
            .as_ref()
            .is_some_and(|resizing| resizing.column == column);
        let grip = theme
            .split_handle(Axis::Horizontal, SplitStyle::Ghost)
            .id(("column-resize", ix))
            .debug_selector(move || format!("table-resize-{ix}"))
            .absolute()
            .top_0()
            .right_0()
            .w(px(COLUMN_RESIZE_HIT))
            .justify_end()
            .group("column-resize")
            .child(
                div()
                    .w(px(1.))
                    .h_full()
                    .bg(if dragging { theme.caret } else { theme.border })
                    .group_hover("column-resize", |line| line.bg(theme.caret)),
            )
            .occlude()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, event: &gpui::MouseDownEvent, _, cx| {
                    this.resizing = Some(Resizing {
                        column: column.clone(),
                        start: event.position.x,
                        width,
                        current: width,
                    });
                    cx.stop_propagation();
                }),
            )
            .on_click(cx.listener(|_, _, _, cx| cx.stop_propagation()))
            .on_drag(ColumnResize(cx.entity().entity_id()), |_, _, _, cx| {
                cx.new(|_| Empty)
            });
        if matches!(&self.cell, Some(Cell::Head(column) | Cell::Option { column, .. }) if column == shape.label.as_ref())
        {
            // `header_cell` paints the label itself, so the shape handed to it
            // while editing carries none — otherwise the old name sits beside
            // the field that is rewriting it.
            let mut blank = shape.clone();
            blank.label = SharedString::default();
            return table::header_cell(&theme, &blank, None)
                .relative()
                .id(shape.label.clone())
                .child(self.cell_editor(cx))
                .child(grip);
        }
        let name = shape.label.to_string();
        let key = TableMenu::Column(name.clone());
        let menu_key = key.clone();
        let menu_name = name.clone();
        let id = SharedString::from(format!("column-menu-{ix}"));
        let button = menu::button(
            id.clone(),
            icons::layout::Ellipsis,
            menu::Reveal::Group("grid-head"),
            self.menu.key() == Some(&key),
            cx,
        )
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .on_click(cx.listener(move |this, _, window, cx| {
            cx.stop_propagation();
            let menu_name = menu_name.clone();
            this.toggle_menu(
                menu_key.clone(),
                Place::Below,
                move |this, _, cx| this.column_rows(menu_name, kind, sorted, cx),
                window,
                cx,
            );
        }));
        let button = self
            .menu_trigger(button, key.clone(), cx)
            .children(self.menu_here(&key, id, window, cx));
        table::header_cell(&theme, shape, sorted)
            .relative()
            .debug_selector(move || format!("table-heading-{ix}"))
            .child(grip)
            .line_height(px(LINE))
            .id(shape.label.clone())
            .group("grid-head")
            .child(button)
            .on_click(cx.listener(move |this, _, window, cx| {
                this.edit_cell(Cell::Head(name.clone()), window, cx);
            }))
    }

    /// What the `···` does to a column: what it holds, and whether it stays.
    fn column_rows(
        &self,
        column: String,
        kind: ColType,
        sorted: Option<bool>,
        cx: &App,
    ) -> Vec<(Item, menu::Act<Self>)> {
        let mut rows = Vec::new();
        for (label, ascending, icon) in [
            ("Sort ascending", true, icons::arrows::ArrowUp),
            ("Sort descending", false, icons::arrows::ArrowDown),
        ] {
            let column = column.clone();
            rows.push(menu::row::<Self>(
                Item::action(label)
                    .with_icon(icon)
                    .checked(sorted == Some(ascending)),
                move |this, _, cx| {
                    this.sort_rows(
                        Some(model::Sort {
                            column: column.clone(),
                            descending: !ascending,
                        }),
                        cx,
                    );
                },
            ));
        }
        if sorted.is_some() {
            rows.push(menu::row::<Self>(
                Item::action("Clear sort"),
                |this, _, cx| this.sort_rows(None, cx),
            ));
        }
        rows.extend(ColType::ALL.iter().map(|declared| {
            let declared = *declared;
            let column = column.clone();
            menu::row::<Self>(
                Item::action(declared.name())
                    .with_icon(glyph(declared))
                    .checked(declared == kind),
                move |this, _, cx| this.retype_column(&column, declared, cx),
            )
        }));
        if kind == ColType::Select {
            let add_column = column.clone();
            let mut options = vec![menu::row::<Self>(
                Item::action("Add option").with_icon(icons::math::Plus),
                move |this, window, cx| {
                    this.edit_cell(
                        Cell::Option {
                            column: add_column.clone(),
                            previous: None,
                        },
                        window,
                        cx,
                    )
                },
            )];
            if let Some(display) = self
                .source
                .read(cx)
                .record
                .column_layout
                .settings
                .get(&column)
            {
                options.extend(display.select_options.iter().map(|option| {
                    let column = column.clone();
                    let option = option.clone();
                    menu::row::<Self>(
                        Item::action(option.clone()).with_icon(icons::text::SquarePen),
                        move |this, window, cx| {
                            this.edit_cell(
                                Cell::Option {
                                    column: column.clone(),
                                    previous: Some(option.clone()),
                                },
                                window,
                                cx,
                            )
                        },
                    )
                }));
            }
            rows.push(menu::submenu("Edit options", glyph(kind), options));
        }
        rows.push(menu::row::<Self>(
            Item::action("Delete column").with_icon(icons::files::Trash),
            move |this, _, cx| this.delete_column(&column, cx),
        ));
        rows
    }

    /// One value: what is stored, or the field when it is being written.
    fn value(
        &self,
        rowid: i64,
        column: &Column,
        held: Option<&Value>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = Theme::of(cx).clone();
        if self.cell
            == Some(Cell::Value {
                rowid,
                column: column.name.clone(),
            })
        {
            return self.cell_editor(cx).into_any_element();
        }
        let contents = held.map(text).unwrap_or_default();
        let select = column.kind == ColType::Select;
        let url = (column.kind == ColType::Url)
            .then(|| url::Url::parse(&contents).ok())
            .flatten()
            .filter(|url| matches!(url.scheme(), "http" | "https"));
        let column = column.name.clone();
        let id = SharedString::from(format!("cell-{rowid}-{column}"));
        let key = TableMenu::Select {
            rowid,
            column: column.clone(),
        };
        let cell = div()
            .id(id.clone())
            .debug_selector({
                let column = column.clone();
                move || format!("table-cell-{rowid}-{column}")
            })
            .w_full()
            .min_h(px(LINE))
            .truncate()
            .cursor_pointer()
            .text_style(TextStyle::Body)
            .line_height(px(LINE))
            .text_color(theme.text)
            .flex()
            .items_center()
            .child(div().min_w_0().flex_1().truncate().child(contents))
            .when(select, |cell| {
                cell.child(
                    icons::icon(icons::arrows::ChevronDown)
                        .size(px(12.))
                        .flex_none()
                        .text_color(theme.text_muted),
                )
            })
            .when_some(url, |cell, url| {
                cell.child(
                    theme
                        .ghost("table-open-url")
                        .flex_none()
                        .child(icons::icon(icons::arrows::ExternalLink).size(px(12.)))
                        .tooltip(|window, cx| Tooltip::text("Open link", window, cx))
                        .on_click(move |_, window, cx| {
                            cx.stop_propagation();
                            link::Links::open(Link::new(url.as_str(), None), window, cx);
                        }),
                )
            })
            .on_click(cx.listener(move |this, _, window, cx| {
                this.edit_cell(
                    Cell::Value {
                        rowid,
                        column: column.clone(),
                    },
                    window,
                    cx,
                );
            }));
        if select {
            self.menu_trigger(cell, key.clone(), cx)
                .children(self.menu_here(&key, id, window, cx))
                .into_any_element()
        } else {
            cell.into_any_element()
        }
    }

    fn row_actions(
        &self,
        rowid: i64,
        revision: Revision,
        manual: bool,
        theme: &Theme,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let remove = theme
            .ghost(SharedString::from(format!("delete-row-{rowid}")))
            .flex_none()
            .invisible()
            .group_hover("grid-row", |el| el.visible())
            .p(px(3.))
            .child(
                icons::icon(icons::files::Trash)
                    .size(px(12.))
                    .text_color(theme.text_faint),
            )
            .on_click(cx.listener(move |this, _, _, cx| {
                cx.stop_propagation();
                this.delete_row(rowid, cx);
            }));
        div()
            .flex()
            .justify_end()
            .gap(px(2.))
            .when(manual, |actions| {
                actions.child(
                    self.row_drag.handle(
                        RowDrag {
                            id: rowid,
                            revision,
                        },
                        theme
                            .ghost(SharedString::from(format!("move-row-{rowid}")))
                            .debug_selector(move || format!("table-move-row-{rowid}"))
                            .flex_none()
                            .group_hover("grid-row", |el| el.text_color(theme.text))
                            .p(px(3.))
                            .text_color(theme.text_faint)
                            .child(icons::icon(icons::arrows::ArrowUpDown).size(px(12.)))
                            .tooltip(|window, cx| Tooltip::text("Move row", window, cx)),
                    ),
                )
            })
            .child(remove)
            .into_any_element()
    }
}

impl Render for TableView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = Theme::of(cx).clone();
        self.offset = self.offset.min(self.source.read(cx).last_page());
        let Some((columns, rows, total, revision)) = self.page(cx).map(|page| {
            (
                page.columns.clone(),
                page.rows
                    .iter()
                    .map(|record| (record.rowid, record.cells.clone()))
                    .collect::<Vec<_>>(),
                page.total,
                page.revision,
            )
        }) else {
            return div()
                .flex_1()
                .px(px(INSET))
                .py(px(20.))
                .text_color(theme.danger)
                .child(
                    self.source
                        .read(cx)
                        .error
                        .clone()
                        .unwrap_or_else(|| "Table could not be loaded".into()),
                )
                .into_any_element();
        };

        let detached = match &self.cell {
            Some(Cell::Value { rowid, column }) => {
                !rows.iter().any(|(id, _)| id == rowid)
                    || !columns.iter().any(|held| held.name == *column)
            }
            Some(Cell::Head(column) | Cell::Option { column, .. }) => {
                !columns.iter().any(|held| held.name == *column)
            }
            _ => false,
        };

        let layout = &self.source.read(cx).record.column_layout;
        let order = layout.order(&columns);
        let (mut declared, width) = self.sizing.columns(
            &columns,
            &order,
            layout,
            self.resizing.as_ref(),
            px(ACTIONS),
        );
        declared.push(table::Column::new("", table::Width::Flex(1.)).align_end());

        let head = div()
            .flex_none()
            .px(px(INSET))
            .py(px(6.))
            .flex()
            .flex_row()
            .items_center()
            .gap(px(8.))
            .child(
                div()
                    .text_style(TextStyle::Subheadline)
                    .text_color(theme.text_faint)
                    .child(match total as usize == rows.len() {
                        true => format!("{total} rows"),
                        false => format!(
                            "{}–{} of {total} rows",
                            self.offset + 1,
                            self.offset + rows.len() as i64
                        ),
                    }),
            )
            .when(total > model::PAGE, |head| {
                head.child(div().flex_1())
                    .child(
                        theme
                            .ghost("table-previous")
                            .debug_selector(|| "table-previous".into())
                            .px(px(8.))
                            .py(px(4.))
                            .text_style(TextStyle::Subheadline)
                            .opacity(if self.offset > 0 { 1. } else { 0.4 })
                            .child("Previous")
                            .when(self.offset > 0, |button| {
                                button.on_click(cx.listener(|this, _, _, cx| {
                                    this.show_page(this.offset - model::PAGE, cx)
                                }))
                            }),
                    )
                    .child(
                        theme
                            .ghost("table-next")
                            .debug_selector(|| "table-next".into())
                            .px(px(8.))
                            .py(px(4.))
                            .text_style(TextStyle::Subheadline)
                            .opacity(if self.offset + model::PAGE < total {
                                1.
                            } else {
                                0.4
                            })
                            .child("Next")
                            .when(self.offset + model::PAGE < total, |button| {
                                button.on_click(cx.listener(|this, _, _, cx| {
                                    this.show_page(this.offset + model::PAGE, cx)
                                }))
                            }),
                    )
            });

        let count = columns.len();
        let mut headings: Vec<AnyElement> = Vec::with_capacity(count + 1);
        for (ix, &index) in order.iter().enumerate() {
            let column = &columns[index];
            let heading = self.heading(ix, column.kind, &declared[ix], window, cx);
            headings.push(
                self.column_drag
                    .handle(column.name.clone(), heading)
                    .into_any_element(),
            );
        }
        headings.push(
            table::header_cell(&theme, &declared[count], None)
                .child(
                    theme
                        .ghost("add-column")
                        .p(px(3.))
                        .child(
                            icons::icon(icons::math::Plus)
                                .size(px(12.))
                                .text_color(theme.text_faint),
                        )
                        .on_click(cx.listener(|this, _, _, cx| this.add_column(cx))),
                )
                .into_any_element(),
        );

        let mut body = div().relative().flex().flex_col();
        let manual = self.source.read(cx).sort.is_none();
        for (n, (rowid, held)) in rows.iter().enumerate() {
            let mut cells: Vec<AnyElement> = Vec::with_capacity(count + 1);
            for &ix in &order {
                cells.push(self.value(*rowid, &columns[ix], held.get(ix), window, cx));
            }
            cells.push(self.row_actions(*rowid, revision, manual, &theme, cx));
            body = body.child(
                table::row(&theme, &declared, n == 0, Selection::None, cells).group("grid-row"),
            );
        }
        let mut edge = px(0.);
        for column in &declared[..count] {
            let table::Width::Fixed(width) = column.width else {
                unreachable!("table columns have measured widths");
            };
            edge += width;
            let dragging = self
                .resizing
                .as_ref()
                .is_some_and(|resize| resize.column == column.label.as_ref());
            body = body.child(
                div()
                    .absolute()
                    .top_0()
                    .bottom_0()
                    .left(edge - px(1.))
                    .w(px(1.))
                    .bg(if dragging {
                        theme.caret
                    } else {
                        theme.border.opacity(0.6)
                    }),
            );
        }
        let grid = table::table(&theme)
            .w(width)
            .min_w_full()
            .max_h_full()
            .child(
                self.column_drag
                    .region(
                        "table-columns",
                        (),
                        Axis::Horizontal,
                        table::header(&theme).children(headings),
                    )
                    .flex_none()
                    .feedback(drag::Feedback::Indicator)
                    .track_scroll(&self.horizontal)
                    .on_drop(cx.listener(Self::move_column)),
            )
            .child(
                scrollbars::pane("table-rows", scrollbars::Axes::Vertical)
                    .min_h_0()
                    .track_scroll(&self.vertical)
                    .child(
                        self.row_drag
                            .region("table-row-order", (), Axis::Vertical, body)
                            .track_scroll(&self.vertical)
                            .feedback(drag::Feedback::Indicator)
                            .on_drop(cx.listener(Self::move_row)),
                    )
                    .child(
                        add::row("add-row", "New row", theme.text_muted, &theme)
                            .mt(px(6.))
                            .px(px(10.))
                            .py(px(7.))
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.add_row(window, cx);
                            })),
                    ),
            );

        div()
            .id("table-view")
            .flex_1()
            .min_w_0()
            .min_h_0()
            .flex()
            .flex_col()
            .on_drag_move(cx.listener(|this, event: &DragMoveEvent<ColumnResize>, _, cx| {
                if event.drag(cx).0 != cx.entity().entity_id() {
                    return;
                }
                if let Some(resizing) = &mut this.resizing {
                    let width = (resizing.width + event.event.position.x - resizing.start)
                        .max(px(MIN_COLUMN_WIDTH));
                    resizing.current = width;
                    cx.notify();
                }
            }))
            .on_mouse_up(MouseButton::Left, cx.listener(|this, _, _, cx| {
                this.finish_resize(cx);
            }))
            .on_mouse_up_out(MouseButton::Left, cx.listener(|this, _, _, cx| {
                this.finish_resize(cx);
            }))
            .on_action(cx.listener(Self::commit_cell_action))
            .on_action(cx.listener(Self::dismiss_cell))
            .child(head)
            .when(detached, |element| element.child(
                div().px(px(INSET)).pb(px(12.)).flex().flex_col().gap(px(8.))
                    .child(div().text_style(TextStyle::Callout).text_color(theme.danger).child("This cell is no longer shown. Copy your text or press Escape to discard it."))
                    .child(self.cell_editor(cx))
            ))
            .when_some(self.source.read(cx).error.clone(), |element, error| {
                element.child(
                    div()
                        .px(px(INSET))
                        .pb(px(12.))
                        .text_style(TextStyle::Callout)
                        .text_color(theme.danger)
                        .child(error),
                )
            })
            .child(
                div()
                    .relative()
                    .flex_1()
                    .min_w_0()
                    .min_h_0()
                    .flex()
                    .flex_col()
                    .child(
                        scrollbars::pane("table-body", scrollbars::Axes::Horizontal)
                            .debug_selector(|| "table-body".into())
                            .flex_1()
                            .min_w_0()
                            .min_h_0()
                            .track_scroll(&self.horizontal)
                            .px(px(INSET))
                            .pb(px(20.))
                            .child(grid),
                    )
                    .child(scrollbars::Overlay::new("table-vertical", &self.vertical, Axis::Vertical))
                    .child(scrollbars::Overlay::new("table-horizontal", &self.horizontal, Axis::Horizontal))
                    .child(self.sizing.measure(INSET, cx)),
            )
            .into_any_element()
    }
}

/// The mark for what a column holds. A menu of four bare labels asks you to
/// read where a glyph would have told you.
fn glyph(kind: ColType) -> &'static [u8] {
    match kind {
        ColType::Text => icons::text::Type,
        ColType::Number => icons::text::Hash,
        ColType::Date => icons::time::Calendar,
        ColType::Check => icons::text::ListChecks,
        ColType::Url => icons::arrows::ExternalLink,
        ColType::Select => icons::text::ListFilter,
    }
}

/// One value as it reads in a cell. An empty cell is drawn as nothing rather
/// than as `null` — a blank is what absence looks like in a grid.
pub(crate) fn text(value: &Value) -> String {
    match value {
        Value::Null => String::new(),
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

#[cfg(all(test, feature = "desktop"))]
#[path = "../../tests/unit/table_editing.rs"]
mod editing_tests;

#[cfg(all(test, feature = "desktop"))]
#[path = "../../tests/unit/table_layout.rs"]
mod layout_tests;
