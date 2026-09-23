//! The table pane: one table's rows, and the sidebar row that opens it.
//!
//! Every edit runs through the one field, the way the board's cards do — a
//! field per cell would mint an entity for every value on screen.

use crate::{
    data::ColType,
    model::workspace::Showing,
    view::{
        component::menu::{self, Menu},
        leaf::Pane,
        root::{Cydonia, NewTable},
        sidebar::{self, Renaming, Row},
    },
};
use artifact::space::Member;
use bezel::ui::scroll as scrollbars;
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
            self.open_table(project, ix, window, cx);
        }
    }

    pub(crate) fn open_table(
        &mut self,
        project: usize,
        ix: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.commit(cx);
        let member = self
            .workspace
            .read(cx)
            .member_of(project, Showing::Table(ix));
        if self.enter_member(member, window, cx) {
            return;
        }
        self.workspace
            .update(cx, |workspace, cx| workspace.open_table(project, ix, cx));
        self.leaf_mut().pane = Pane::Table;
        cx.notify();
    }

    /// The table the focused pane is on, by its key — the one address every
    /// write to a table is made through. A window with no space open has no
    /// member to name, and falls back to what its project is pointed at.
    pub(crate) fn pane_table(&self, cx: &App) -> Option<String> {
        self.workspace
            .read(cx)
            .table_of(self.leaf().entry.as_ref())
            .map(|table| table.key.clone())
    }

    /// The rows that pane is showing.
    fn pane_page<'a>(&self, cx: &'a App) -> Option<&'a crate::data::Page> {
        let key = self.pane_table(cx)?;
        self.workspace.read(cx).page_at(&key)
    }

    /// Point the field at `at`, filing whatever was already open first — so
    /// clicking straight from one cell to another never drops an edit.
    fn edit_cell(&mut self, at: Cell, window: &mut Window, cx: &mut Context<Self>) {
        self.commit(cx);
        let text = match at {
            Cell::Value { rowid, column } => self
                .pane_page(cx)
                .and_then(|page| page.rows.iter().find(|record| record.rowid == rowid))
                .and_then(|record| record.cells.get(column))
                .map(text)
                .unwrap_or_default(),
            Cell::Head(ix) => self
                .pane_page(cx)
                .and_then(|page| page.columns.get(ix))
                .map(|column| column.name.clone())
                .unwrap_or_default(),
            Cell::Name => self
                .workspace
                .read(cx)
                .table_of(self.leaf().entry.as_ref())
                .map(|table| table.name.clone())
                .unwrap_or_default(),
        };
        self.leaf()
            .cell_field
            .update(cx, |field, cx| field.set_content(text, cx));
        self.leaf_mut().cell = Some(at);
        window.focus(&self.leaf().cell_field.read(cx).focus_handle(cx), cx);
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
        let Some(at) = self.leaf_mut().cell.take() else {
            return;
        };
        let text = self.leaf().cell_field.read(cx).content().trim().to_owned();
        self.leaf()
            .cell_field
            .update(cx, |field, cx| field.clear(cx));
        let Some(key) = self.pane_table(cx) else {
            return;
        };
        self.workspace.update(cx, |workspace, cx| match at {
            Cell::Value { rowid, column } => workspace.write_cell(&key, rowid, column, text, cx),
            Cell::Head(ix) if !text.is_empty() => {
                workspace.write_column(&key, ix, None, Some(text), cx);
            }
            Cell::Name if !text.is_empty() => workspace.rename_table(&key, text, cx),
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
        self.leaf_mut().cell = None;
        self.leaf()
            .cell_field
            .update(cx, |field, cx| field.clear(cx));
        cx.notify();
    }

    /// A row, and the caret in the first cell of it — an empty row you have to
    /// go and click is two actions for one intent.
    fn add_row(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.commit(cx);
        let Some(key) = self.pane_table(cx) else {
            return;
        };
        let rowid = self
            .workspace
            .update(cx, |workspace, cx| workspace.add_row(&key, cx));
        if let Some(rowid) = rowid {
            self.edit_cell(Cell::Value { rowid, column: 0 }, window, cx);
        }
    }

    fn delete_row(&mut self, rowid: i64, cx: &mut Context<Self>) {
        self.commit(cx);
        let Some(key) = self.pane_table(cx) else {
            return;
        };
        self.workspace
            .update(cx, |workspace, cx| workspace.delete_row(&key, rowid, cx));
    }

    fn add_column(&mut self, cx: &mut Context<Self>) {
        self.commit(cx);
        let Some(key) = self.pane_table(cx) else {
            return;
        };
        self.workspace
            .update(cx, |workspace, cx| workspace.add_column(&key, cx));
    }

    fn retype_column(&mut self, at: usize, kind: ColType, cx: &mut Context<Self>) {
        let Some(key) = self.pane_table(cx) else {
            return;
        };
        self.workspace.update(cx, |workspace, cx| {
            workspace.write_column(&key, at, Some(kind), None, cx)
        });
    }

    fn delete_column(&mut self, at: usize, cx: &mut Context<Self>) {
        let Some(key) = self.pane_table(cx) else {
            return;
        };
        self.workspace
            .update(cx, |workspace, cx| workspace.delete_column(&key, at, cx));
    }

    // ── chrome ───────────────────────────────────────────────────

    /// The table. Same frame as [`Cydonia::article`]: the body of the content
    /// card, with the composer stack still pinned under it.
    pub(crate) fn table(
        &self,
        project: usize,
        at: usize,
        on: Option<&Member>,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let theme = Theme::of(cx).clone();
        let (name, columns, rows, total) = {
            let page = self.workspace.read(cx).page_in(project, at)?;
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
            .child(match self.leaf_of(on).cell == Some(Cell::Name) {
                true => div()
                    .w(px(240.))
                    .child(self.cell_editor(on, cx))
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
            headings.push(self.heading(ix, column.kind, &declared[ix], on, cx));
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
            for ix in 0..count {
                cells.push(self.value(*rowid, ix, held.get(ix), on, cx));
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
                                    icons::icon(icons::math::Plus)
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
                        )
                        .map(|pane| {
                            scrollbars::Viewport::new(
                                "table-scroll",
                                pane,
                                bezel::gpui::Axis::Vertical,
                            )
                            .fill()
                        }),
                )
                .into_any_element(),
        )
    }

    /// The field, wired so a click anywhere else files it.
    ///
    /// gpui holds focus until something takes it, and `TextField` carries no
    /// blur policy of its own — it cannot know whether leaving means commit or
    /// cancel. Here it means commit, the same as `enter`.
    fn cell_editor(&self, on: Option<&Member>, cx: &mut Context<Self>) -> Div {
        div()
            .w_full()
            .child(self.leaf_of(on).cell_field.clone())
            .on_mouse_down_out(cx.listener(|this, _, _, cx| this.commit_cell(cx)))
    }

    /// One heading: the column's name, and the menu that changes what it is.
    fn heading(
        &self,
        ix: usize,
        kind: ColType,
        shape: &table::Column,
        on: Option<&Member>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = Theme::of(cx).clone();
        if self.leaf_of(on).cell == Some(Cell::Head(ix)) {
            // `header_cell` paints the label itself, so the shape handed to it
            // while editing carries none — otherwise the old name sits beside
            // the field that is rewriting it.
            let mut blank = shape.clone();
            blank.label = SharedString::default();
            return table::header_cell(&theme, &blank, None)
                .child(self.cell_editor(on, cx))
                .into_any_element();
        }
        table::header_cell(&theme, shape, None)
            .line_height(px(LINE))
            .id(("heading", ix))
            .group("grid-head")
            .child(
                self.menu_button(
                    SharedString::from(format!("column-menu-{ix}")),
                    Some("grid-head"),
                    icons::layout::Ellipsis,
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
            Item::action("Delete column").with_icon(icons::files::Trash),
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
        on: Option<&Member>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = Theme::of(cx).clone();
        if self.leaf_of(on).cell == Some(Cell::Value { rowid, column }) {
            return self.cell_editor(on, cx).into_any_element();
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

    /// One table in the sidebar, under the project that holds it.
    pub(crate) fn table_row(
        &self,
        project: usize,
        ix: usize,
        name: String,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let theme = Theme::of(cx).clone();
        let workspace = self.workspace.read(cx);
        let selected = !self.arranged(cx)
            && self.showing(cx) == Some(Pane::Table)
            && workspace.active == Some(project)
            && workspace
                .projects
                .get(project)
                .is_some_and(|open| open.table == Some(ix));
        let entry = Row::Table { project, ix };
        let table = workspace
            .projects
            .get(project)
            .and_then(|open| open.tables.get(ix));
        let archived = table.is_some_and(|table| table.archived);
        let key = table.map(|table| &table.key);
        // The band draws the field when it is showing this entry — see
        // [`Cydonia::header_renaming`], which is what keeps one field from
        // being claimed by two places at once.
        let renaming = matches!(&self.renaming, Some(Renaming::Table(at)) if Some(at) == key)
            && self.header_renaming(cx).is_none();
        let tone = sidebar::tint(selected, archived, &theme);

        sidebar::row(
            SharedString::from(format!("table-{project}-{ix}")),
            "table-row",
            selected,
            self.indent_of(entry, cx),
            &theme,
        )
        .child(
            icons::icon(icons::files::Table2)
                .size(px(14.))
                .flex_none()
                .text_color(tone),
        )
        .child(match renaming {
            true => self.name_field(cx),
            false => div()
                .flex_1()
                .min_w_0()
                .truncate()
                .text_style(TextStyle::Body)
                .text_color(tone)
                .child(name)
                .into_any_element(),
        })
        .child(self.archive_button(format!("table-archive-{ix}"), "table-row", entry, archived, cx))
        .on_click(cx.listener(move |this, _, window, cx| {
            this.open_table(project, ix, window, cx);
        }))
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
fn text(value: &Value) -> String {
    match value {
        Value::Null => String::new(),
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}
