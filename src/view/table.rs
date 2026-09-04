//! The table pane: one table's rows, and the sidebar row that opens it.
//!
//! Every edit runs through the one field, the way the board's cards do — a
//! field per cell would mint an entity for every value on screen.

use crate::{
    data::ColType,
    view::{
        component::menu::{self, Menu},
        root::{Cydonia, Pane},
        sidebar,
    },
};
use bezel::{
    gpui::{
        self, AnyElement, App, Context, Div, Entity, Focusable as _, KeyBinding, SharedString,
        Window, actions, div, prelude::*, px,
    },
    theme::{TextStyle, Theme, Typeset},
    ui::{
        icons,
        input::{Shape, TextField},
        menu::Item,
        popover, table,
        widgets::Buttons,
    },
};
use serde_json::Value;

actions!(cydonia_table, [CommitCell, DismissCell]);

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

pub fn init(cx: &mut App) {
    let ctx = Some(KEY_CONTEXT);
    cx.bind_keys([
        KeyBinding::new("enter", CommitCell, ctx),
        KeyBinding::new("escape", DismissCell, ctx),
    ]);
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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cell {
    /// A value being written, in the row with this id.
    Value { rowid: i64, column: usize },
    /// A column being renamed.
    Head(usize),
    /// The table being renamed.
    Name,
}

impl Cydonia {
    // ── mutations ────────────────────────────────────────────────

    pub(crate) fn new_table(&mut self, project: usize, cx: &mut Context<Self>) {
        self.select_project(project, cx);
        let ix = self
            .workspace
            .update(cx, |workspace, cx| workspace.new_table(cx));
        if let Some(ix) = ix {
            self.open_table(project, ix, cx);
        }
    }

    pub(crate) fn open_table(&mut self, project: usize, ix: usize, cx: &mut Context<Self>) {
        self.commit(cx);
        self.workspace
            .update(cx, |workspace, cx| workspace.open_table(project, ix, cx));
        self.pane = Pane::Table;
        cx.notify();
    }

    fn delete_table(&mut self, project: usize, ix: usize, cx: &mut Context<Self>) {
        self.workspace.update(cx, |workspace, cx| {
            workspace.delete_table(project, ix, cx);
        });
        cx.notify();
    }

    /// Point the field at `at`, filing whatever was already open first — so
    /// clicking straight from one cell to another never drops an edit.
    fn edit_cell(&mut self, at: Cell, window: &mut Window, cx: &mut Context<Self>) {
        self.commit(cx);
        let workspace = self.workspace.read(cx);
        let text = match at {
            Cell::Value { rowid, column } => workspace
                .active_page()
                .and_then(|page| page.rows.iter().find(|record| record.rowid == rowid))
                .and_then(|record| record.cells.get(column))
                .map(text)
                .unwrap_or_default(),
            Cell::Head(ix) => workspace
                .active_page()
                .and_then(|page| page.columns.get(ix))
                .map(|column| column.name.clone())
                .unwrap_or_default(),
            Cell::Name => workspace
                .active_table()
                .map(|table| table.name.clone())
                .unwrap_or_default(),
        };
        self.cell_field
            .update(cx, |field, cx| field.set_content(text, cx));
        self.cell = Some(at);
        window.focus(&self.cell_field.read(cx).focus_handle(cx), cx);
        cx.notify();
    }

    /// File whatever the field is attached to. Called from
    /// [`Cydonia::commit`], so every way out of the pane goes through it.
    ///
    /// A heading and a title cannot be emptied — a column with no name is not
    /// addressable and a table with none has nothing to show in the sidebar — so
    /// an empty one is a cancel. A *cell* may be emptied: that is how a value
    /// is cleared.
    pub(crate) fn commit_cell(&mut self, cx: &mut Context<Self>) {
        let Some(at) = self.cell.take() else {
            return;
        };
        let text = self.cell_field.read(cx).content().trim().to_owned();
        self.cell_field.update(cx, |field, cx| field.clear(cx));
        self.workspace.update(cx, |workspace, cx| match at {
            Cell::Value { rowid, column } => workspace.write_cell(rowid, column, text, cx),
            Cell::Head(ix) if !text.is_empty() => {
                workspace.write_column(ix, None, Some(text), cx);
            }
            Cell::Name if !text.is_empty() => workspace.rename_table(text, cx),
            _ => {}
        });
        cx.notify();
    }

    pub(crate) fn commit_cell_action(
        &mut self,
        _: &CommitCell,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.commit_cell(cx);
    }

    /// Drop the edit and leave what was there.
    pub(crate) fn dismiss_cell(&mut self, _: &DismissCell, _: &mut Window, cx: &mut Context<Self>) {
        self.cell = None;
        self.cell_field.update(cx, |field, cx| field.clear(cx));
        cx.notify();
    }

    /// A row, and the caret in the first cell of it — an empty row you have to
    /// go and click is two actions for one intent.
    fn add_row(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.commit(cx);
        let rowid = self
            .workspace
            .update(cx, |workspace, cx| workspace.add_row(cx));
        if let Some(rowid) = rowid {
            self.edit_cell(Cell::Value { rowid, column: 0 }, window, cx);
        }
    }

    fn delete_row(&mut self, rowid: i64, cx: &mut Context<Self>) {
        self.commit(cx);
        self.workspace
            .update(cx, |workspace, cx| workspace.delete_row(rowid, cx));
    }

    fn add_column(&mut self, cx: &mut Context<Self>) {
        self.commit(cx);
        self.workspace
            .update(cx, |workspace, cx| workspace.add_column(cx));
    }

    fn retype_column(&mut self, at: usize, kind: ColType, cx: &mut Context<Self>) {
        self.workspace.update(cx, |workspace, cx| {
            workspace.write_column(at, Some(kind), None, cx)
        });
    }

    fn delete_column(&mut self, at: usize, cx: &mut Context<Self>) {
        self.workspace
            .update(cx, |workspace, cx| workspace.delete_column(at, cx));
    }

    // ── chrome ───────────────────────────────────────────────────

    /// The table. Same frame as [`Cydonia::article`]: the body of the content
    /// card, with the composer stack still pinned under it.
    pub(crate) fn table(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let theme = Theme::of(cx).clone();
        let (name, columns, rows, total) = {
            let page = self.workspace.read(cx).active_page()?;
            (
                page.name.clone(),
                page.columns.clone(),
                page.rows
                    .iter()
                    .map(|record| (record.rowid, record.cells.clone()))
                    .collect::<Vec<_>>(),
                page.total,
            )
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
            headings.push(self.heading(ix, column.kind, &declared[ix], cx));
        }
        headings.push(
            table::header_cell(&theme, &declared[count], None)
                .child(
                    theme
                        .ghost("add-column")
                        .p(px(3.))
                        .child(
                            icons::icon(icons::PLUS)
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
            for ix in 0..count {
                cells.push(self.value(*rowid, ix, held.get(ix), cx));
            }
            cells.push(self.row_actions(*rowid, &theme, cx));
            body =
                body.child(table::row(&theme, &declared, n == 0, false, cells).group("grid-row"));
        }

        Some(
            div()
                .flex_1()
                .min_h_0()
                .flex()
                .flex_col()
                .child(head)
                .child(
                    div()
                        .id("table-body")
                        .flex_1()
                        .min_h_0()
                        .overflow_y_scroll()
                        .px(px(24.))
                        .pb(px(20.))
                        .child(body)
                        .child(
                            theme
                                .ghost("add-row")
                                .mt(px(6.))
                                .px(px(10.))
                                .py(px(7.))
                                .gap(px(6.))
                                .child(
                                    icons::icon(icons::PLUS)
                                        .size(px(12.))
                                        .text_color(theme.text_faint),
                                )
                                .child(
                                    div()
                                        .text_style(TextStyle::Callout)
                                        .text_color(theme.text_muted)
                                        .child("New row"),
                                )
                                .on_click(
                                    cx.listener(|this, _, window, cx| this.add_row(window, cx)),
                                ),
                        ),
                )
                .into_any_element(),
        )
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
            .on_mouse_down_out(cx.listener(|this, _, _, cx| this.commit_cell(cx)))
    }

    /// One heading: the column's name, and the menu that changes what it is.
    fn heading(
        &self,
        ix: usize,
        kind: ColType,
        shape: &table::Column,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = Theme::of(cx).clone();
        if self.cell == Some(Cell::Head(ix)) {
            // `header_cell` paints the label itself, so the shape handed to it
            // while editing carries none — otherwise the old name sits beside
            // the field that is rewriting it.
            let mut blank = shape.clone();
            blank.label = SharedString::default();
            return table::header_cell(&theme, &blank, None)
                .child(self.cell_editor(cx))
                .into_any_element();
        }
        table::header_cell(&theme, shape, None)
            .line_height(px(LINE))
            .id(("heading", ix))
            .group("grid-head")
            .child(
                self.menu_button(
                    ("column-menu", ix),
                    "grid-head",
                    icons::icon(icons::MENU_DOTS)
                        .size(px(14.))
                        .text_color(theme.text_faint),
                    Menu::Column(ix),
                    cx,
                )
                .children(self.column_menu(ix, kind, cx)),
            )
            .on_click(cx.listener(move |this, _, window, cx| {
                this.edit_cell(Cell::Head(ix), window, cx);
            }))
            .into_any_element()
    }

    /// What the `···` does to a column: what it holds, and whether it stays.
    fn column_menu(&self, ix: usize, kind: ColType, cx: &mut Context<Self>) -> Option<AnyElement> {
        if self.menu != Some(Menu::Column(ix)) {
            return None;
        }
        let mut rows: Vec<_> = ColType::ALL
            .iter()
            .map(|declared| {
                let declared = *declared;
                menu::row(
                    Item::action(declared.name())
                        .with_icon(glyph(declared))
                        .checked(declared == kind),
                    move |this, _, cx| this.retype_column(ix, declared, cx),
                )
            })
            .collect();
        rows.push(menu::row(
            Item::action("Delete column").with_icon(icons::TRASH_BIN_MINIMALISTIC),
            move |this, _, cx| this.delete_column(ix, cx),
        ));
        let id = SharedString::from(format!("column-menu-{ix}"));
        Some(popover::anchored_menu_below(
            id.clone(),
            self.menu_card(id, rows, cx),
            None,
        ))
    }

    /// One value: what is stored, or the field when it is being written.
    fn value(
        &self,
        rowid: i64,
        column: usize,
        held: Option<&Value>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = Theme::of(cx).clone();
        if self.cell == Some(Cell::Value { rowid, column }) {
            return self.cell_editor(cx).into_any_element();
        }
        div()
            .id(SharedString::from(format!("cell-{rowid}-{column}")))
            .w_full()
            .truncate()
            .cursor_pointer()
            .text_style(TextStyle::Body)
            .line_height(px(LINE))
            .text_color(theme.text)
            .child(held.map(text).unwrap_or_default())
            .on_click(cx.listener(move |this, _, window, cx| {
                this.edit_cell(Cell::Value { rowid, column }, window, cx);
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
                icons::icon(icons::TRASH_BIN_MINIMALISTIC)
                    .size(px(12.))
                    .text_color(theme.text_faint),
            )
            .on_click(cx.listener(move |this, _, _, cx| {
                cx.stop_propagation();
                this.delete_row(rowid, cx);
            }))
            .into_any_element()
    }

    /// One table in the sidebar, under the project that holds it.
    pub(crate) fn table_row(
        &self,
        project: usize,
        ix: usize,
        name: String,
        cx: &Context<Self>,
    ) -> impl IntoElement + use<> {
        let theme = Theme::of(cx).clone();
        let workspace = self.workspace.read(cx);
        let selected = self.showing(cx) == Pane::Table
            && workspace.active == Some(project)
            && workspace
                .projects
                .get(project)
                .is_some_and(|open| open.table == Some(ix));
        let tone = if selected {
            theme.text
        } else {
            theme.text_muted
        };

        sidebar::row(
            SharedString::from(format!("table-{project}-{ix}")),
            "table-row",
            selected,
            &theme,
        )
        .child(
            icons::icon(icons::WIDGET)
                .size(px(14.))
                .flex_none()
                .text_color(tone),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .truncate()
                .text_style(TextStyle::Body)
                .text_color(tone)
                .child(name),
        )
        .child(
            theme
                .ghost(("delete-table", ix))
                .flex_none()
                .invisible()
                .group_hover("table-row", |el| el.visible())
                .p(px(2.))
                .child(
                    icons::icon(icons::TRASH_BIN_MINIMALISTIC)
                        .size(px(12.))
                        .text_color(theme.text_faint),
                )
                .on_click(cx.listener(move |this, _, _, cx| {
                    cx.stop_propagation();
                    this.delete_table(project, ix, cx);
                })),
        )
        .on_click(cx.listener(move |this, _, _, cx| {
            this.open_table(project, ix, cx);
        }))
    }
}

/// The mark for what a column holds. A menu of four bare labels asks you to
/// read where a glyph would have told you.
fn glyph(kind: ColType) -> &'static str {
    match kind {
        ColType::Text => icons::TEXT,
        ColType::Number => icons::HASHTAG,
        ColType::Date => icons::CALENDAR,
        ColType::Check => icons::CHECKLIST,
    }
}

/// One value as it reads in a cell. An empty cell is drawn as nothing rather
/// than as `null` — a blank is what absence looks like in a grid.
fn text(value: &Value) -> String {
    match value {
        Value::Null => String::new(),
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}
