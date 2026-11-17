//! The table pane: one table's rows, and the sidebar row that opens it.
//!
use crate::{
    data::ColType,
    model::{
        table::Table,
        workspace::{Entry, Member, Opening, Workspace},
    },
    view::{
        kit::{
            add,
            menu::{self, MenuHost},
        },
        window::root::{Cydonia, NewTable},
    },
};
use bezel::{
    gpui::{
        self, AnyElement, App, Context, Div, Entity, Focusable as _, KeyBinding, SharedString,
        WeakEntity, Window, actions, div, prelude::*, px,
    },
    theme::{Selection, TextStyle, Theme, Typeset},
    ui::{
        context_menu::{ContextMenu, Place},
        icons,
        input::{Shape, TextField},
        menu::Item,
        scroll as scrollbars, table,
        widgets::Buttons,
    },
};
use serde_json::Value;

/// Claimed on top of `TextField`, so `enter` files the cell here and stays a
/// newline in every other field.
const KEY_CONTEXT: &str = "CydoniaCell";

/// The line box every cell keeps, focused or not. It is what `TextField`
/// renders at and cannot be told otherwise, so a cell that sized itself to its
/// own text would grow the row the moment you clicked into it.
const LINE: f32 = 18.;

/// The column the row actions sit in — the trash on a row, the `+` on the
/// header. Declared with the data columns so both halves line up.
const ACTIONS: f32 = 44.;

actions!(cydonia_table, [CommitCell, DismissCell]);

pub fn bindings() -> Vec<KeyBinding> {
    let ctx = Some(KEY_CONTEXT);
    vec![
        KeyBinding::new("enter", CommitCell, ctx),
        KeyBinding::new("escape", DismissCell, ctx),
    ]
}

/// The pane's one text field — whichever cell, heading or title is being
/// written.
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
    /// The table being renamed.
    Name,
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
        self.select_project(project, cx);
        let ix = self
            .workspace
            .update(cx, |workspace, cx| workspace.new_table(cx));
        if let Some(ix) = ix {
            self.open_table(project, ix, Opening::Kept, window, cx);
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
        self.commit(cx);
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
    pub(crate) fn commit_cell(&mut self, cx: &mut Context<Self>) {
        for view in self.table_views(cx) {
            view.update(cx, |view, cx| view.commit_cell(cx));
        }
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
                TablePreview { source }
            },
        );
        Some(view.into_any_element())
    }
}

struct TablePreview {
    source: Entity<Table>,
}

impl Render for TablePreview {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = Theme::of(cx).clone();
        let source = self.source.read(cx);
        let Some(page) = source.page.as_ref() else {
            return div()
                .text_color(theme.danger)
                .child(source.error.clone().unwrap_or_default())
                .into_any_element();
        };
        let declared: Vec<table::Column> = page
            .columns
            .iter()
            .map(|column| {
                let shape = table::Column::new(column.name.clone(), table::Width::Flex(1.));
                match column.kind {
                    ColType::Number => shape.align_end(),
                    _ => shape,
                }
            })
            .collect();
        let headings = declared
            .iter()
            .map(|column| table::header_cell(&theme, column, None).cursor_default());
        let mut body = table::table(&theme).child(table::header(&theme).children(headings));
        for (n, record) in page.rows.iter().enumerate() {
            let cells = (0..declared.len())
                .map(|ix| {
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
            .id("table-peek")
            .flex_1()
            .min_h_0()
            .map(|el| scrollbars::scrolls(el, scrollbars::Axes::Vertical))
            .px(px(16.))
            .py(px(12.))
            .flex()
            .flex_col()
            .gap(px(8.))
            .child(body)
            .child(
                div()
                    .text_style(TextStyle::Subheadline)
                    .text_color(theme.text_faint)
                    .child(count),
            )
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
    root: WeakEntity<Cydonia>,
    /// What the field is attached to, and the field itself.
    pub(crate) cell: Option<Cell>,
    pub(crate) cell_field: Entity<TextField>,
    /// The column menu that is up, by column.
    menu: ContextMenu<Self, usize>,
}

impl MenuHost for TableView {
    type Key = usize;

    fn menu(&self) -> &ContextMenu<Self, usize> {
        &self.menu
    }

    fn menu_mut(&mut self) -> &mut ContextMenu<Self, usize> {
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
            root,
            cell: None,
            cell_field: field(cx),
            menu: ContextMenu::default(),
        }
    }

    fn page<'a>(&self, cx: &'a App) -> Option<&'a crate::data::Page> {
        self.source.read(cx).page.as_ref()
    }

    /// Point the field at `at`, filing whatever any pane had open first — so
    /// clicking straight from one cell to another never drops an edit.
    fn edit_cell(&mut self, at: Cell, window: &mut Window, cx: &mut Context<Self>) {
        let (root, view) = (self.root.clone(), cx.entity());
        window.defer(cx, move |window, cx| {
            let _ = root.update(cx, |root, cx| root.commit(cx));
            view.update(cx, |view, cx| {
                if view.cell.is_none() {
                    view.start_edit(at, window, cx);
                }
            });
        });
    }

    fn start_edit(&mut self, at: Cell, window: &mut Window, cx: &mut Context<Self>) {
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
            Cell::Name => self.source.read(cx).record.name.clone(),
        };
        self.cell_field
            .update(cx, |field, cx| field.set_content(text, cx));
        self.cell = Some(at);
        window.focus(&self.cell_field.read(cx).focus_handle(cx), cx);
        cx.notify();
    }

    pub(crate) fn commit_cell(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(at) = self.cell.clone() else {
            return true;
        };
        let text = self.cell_field.read(cx).content().trim().to_owned();
        let entry = self.table.clone();
        let saved = self.workspace.update(cx, |workspace, cx| {
            workspace.edit_table(&entry, cx, |table, cx| match at {
                Cell::Value { rowid, column } => table.write_cell(rowid, column, text, cx),
                Cell::Head(column) if !text.is_empty() => {
                    table.write_column(&column, None, Some(&text), cx)
                }
                Cell::Name if !text.is_empty() => table.rename(&text, cx),
                _ => Ok(()),
            })
        });
        if saved.is_ok() {
            self.cell = None;
            self.cell_field.update(cx, |field, cx| field.clear(cx));
        }
        cx.notify();
        saved.is_ok()
    }

    /// Let go of the field without filing it.
    fn drop_cell(&mut self, cx: &mut Context<Self>) {
        self.cell = None;
        self.cell_field.update(cx, |field, cx| field.clear(cx));
        cx.notify();
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

    /// One heading: the column's name, and the menu that changes what it is.
    fn heading(
        &self,
        ix: usize,
        kind: ColType,
        shape: &table::Column,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = Theme::of(cx).clone();
        if self.cell == Some(Cell::Head(shape.label.to_string())) {
            // `header_cell` paints the label itself, so the shape handed to it
            // while editing carries none — otherwise the old name sits beside
            // the field that is rewriting it.
            let mut blank = shape.clone();
            blank.label = SharedString::default();
            return table::header_cell(&theme, &blank, None)
                .child(self.cell_editor(cx))
                .into_any_element();
        }
        let name = shape.label.to_string();
        let menu_name = name.clone();
        let id = SharedString::from(format!("column-menu-{ix}"));
        let button = menu::button(
            id.clone(),
            icons::layout::Ellipsis,
            menu::Reveal::Group("grid-head"),
            self.menu.key() == Some(&ix),
            cx,
        )
        .on_click(cx.listener(move |this, _, window, cx| {
            cx.stop_propagation();
            let menu_name = menu_name.clone();
            this.toggle_menu(
                ix,
                Place::Below,
                move |_, _, _| Self::column_rows(menu_name.clone(), kind),
                window,
                cx,
            );
        }));
        let button = self
            .menu_trigger(button, ix, cx)
            .children(self.menu_here(&ix, id, window, cx));
        table::header_cell(&theme, shape, None)
            .line_height(px(LINE))
            .id(("heading", ix))
            .group("grid-head")
            .child(button)
            .on_click(cx.listener(move |this, _, window, cx| {
                this.edit_cell(Cell::Head(name.clone()), window, cx);
            }))
            .into_any_element()
    }

    /// What the `···` does to a column: what it holds, and whether it stays.
    fn column_rows(column: String, kind: ColType) -> Vec<(Item, menu::Act<Self>)> {
        let mut rows: Vec<_> = ColType::ALL
            .iter()
            .map(|declared| {
                let declared = *declared;
                let column = column.clone();
                menu::row::<Self>(
                    Item::action(declared.name())
                        .with_icon(glyph(declared))
                        .checked(declared == kind),
                    move |this, _, cx| this.retype_column(&column, declared, cx),
                )
            })
            .collect();
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
        column: &str,
        held: Option<&Value>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = Theme::of(cx).clone();
        if self.cell
            == Some(Cell::Value {
                rowid,
                column: column.to_owned(),
            })
        {
            return self.cell_editor(cx).into_any_element();
        }
        let column = column.to_owned();
        div()
            .id(SharedString::from(format!("cell-{rowid}-{column}")))
            .w_full()
            .min_h(px(LINE))
            .truncate()
            .cursor_pointer()
            .text_style(TextStyle::Body)
            .line_height(px(LINE))
            .text_color(theme.text)
            .child(held.map(text).unwrap_or_default())
            .on_click(cx.listener(move |this, _, window, cx| {
                this.edit_cell(
                    Cell::Value {
                        rowid,
                        column: column.clone(),
                    },
                    window,
                    cx,
                );
            }))
            .into_any_element()
    }

    fn row_actions(&self, rowid: i64, theme: &Theme, cx: &mut Context<Self>) -> AnyElement {
        theme
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
            }))
            .into_any_element()
    }
}

impl Render for TableView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = Theme::of(cx).clone();
        let Some((name, columns, rows, total)) = self.page(cx).map(|page| {
            (
                page.name.clone(),
                page.columns.clone(),
                page.rows
                    .iter()
                    .map(|record| (record.rowid, record.cells.clone()))
                    .collect::<Vec<_>>(),
                page.total,
            )
        }) else {
            return div()
                .flex_1()
                .px(px(24.))
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
            Some(Cell::Head(column)) => !columns.iter().any(|held| held.name == *column),
            _ => false,
        };

        let mut declared: Vec<table::Column> = columns
            .iter()
            .map(|column| {
                let shape = table::Column::new(column.name.clone(), table::Width::Flex(1.));
                match column.kind {
                    ColType::Number => shape.align_end(),
                    _ => shape,
                }
            })
            .collect();
        declared.push(table::Column::new("", table::Width::Fixed(px(ACTIONS))));

        let head = div()
            .flex_none()
            .px(px(24.))
            .pt(px(20.))
            .pb(px(12.))
            .flex()
            .flex_row()
            .items_baseline()
            .gap(px(8.))
            .child(match self.cell == Some(Cell::Name) {
                true => div()
                    .w(px(240.))
                    .child(self.cell_editor(cx))
                    .into_any_element(),
                false => div()
                    .id("table-name")
                    .cursor_pointer()
                    .text_style(TextStyle::Title3)
                    .text_color(theme.text)
                    .child(name)
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.edit_cell(Cell::Name, window, cx);
                    }))
                    .into_any_element(),
            })
            .child(
                div()
                    .text_style(TextStyle::Subheadline)
                    .text_color(theme.text_faint)
                    // The table's own count, not the window's — a pane that
                    // stops at its limit without saying so reads as the end.
                    .child(match total as usize == rows.len() {
                        true => format!("{total} rows"),
                        false => format!("{} of {total} rows", rows.len()),
                    }),
            );

        let count = columns.len();
        let mut headings: Vec<AnyElement> = Vec::with_capacity(count + 1);
        for (ix, column) in columns.iter().enumerate() {
            headings.push(self.heading(ix, column.kind, &declared[ix], window, cx));
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

        let mut body = table::table(&theme).child(table::header(&theme).children(headings));
        for (n, (rowid, held)) in rows.iter().enumerate() {
            let mut cells: Vec<AnyElement> = Vec::with_capacity(count + 1);
            for (ix, column) in columns.iter().enumerate() {
                cells.push(self.value(*rowid, &column.name, held.get(ix), cx));
            }
            cells.push(self.row_actions(*rowid, &theme, cx));
            body = body.child(
                table::row(&theme, &declared, n == 0, Selection::None, cells).group("grid-row"),
            );
        }

        div()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .on_action(cx.listener(Self::commit_cell_action))
            .on_action(cx.listener(Self::dismiss_cell))
            .child(head)
            .when(detached, |element| element.child(
                div().px(px(24.)).pb(px(12.)).flex().flex_col().gap(px(8.))
                    .child(div().text_style(TextStyle::Callout).text_color(theme.danger).child("This cell is no longer shown. Copy your text or press Escape to discard it."))
                    .child(self.cell_editor(cx))
            ))
            .when_some(self.source.read(cx).error.clone(), |element, error| {
                element.child(
                    div()
                        .px(px(24.))
                        .pb(px(12.))
                        .text_style(TextStyle::Callout)
                        .text_color(theme.danger)
                        .child(error),
                )
            })
            .child(
                div()
                    .id("table-body")
                    .flex_1()
                    .min_h_0()
                    .map(|el| scrollbars::scrolls(el, scrollbars::Axes::Vertical))
                    .px(px(24.))
                    .pb(px(20.))
                    .child(body)
                    .child(
                        add::row("add-row", "New row", theme.text_muted, &theme)
                            .mt(px(6.))
                            .px(px(10.))
                            .py(px(7.))
                            .on_click(cx.listener(|this, _, window, cx| this.add_row(window, cx))),
                    )
                    .map(|pane| {
                        scrollbars::Viewport::new("table-scroll", pane, bezel::gpui::Axis::Vertical)
                            .fill()
                    }),
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
