use crate::{
    data::{Column, Revision},
    model::table::{self, Sort},
    view::table::{Cell, KEY_CONTEXT, TableView},
};
use bezel::{
    gpui::{
        self, AnyElement, Bounds, Context, EntityId, FocusHandle, KeyBinding, Pixels, Window,
        prelude::*,
    },
    ui::input,
};
use std::ops::RangeInclusive;

pub(super) const CONTEXT: &str = "CydoniaTableGrid";

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Address {
    pub(super) rowid: i64,
    pub(super) row: i64,
    pub(super) column: String,
}

#[derive(Clone)]
pub(super) struct Range {
    pub(super) anchor: Address,
    pub(super) active: Address,
    pub(super) revision: Revision,
    pub(super) sort: Option<Sort>,
}

impl Range {
    pub(super) fn current(&self, revision: Revision, sort: Option<&Sort>) -> bool {
        self.revision == revision && self.sort.as_ref() == sort
    }

    pub(super) fn limits(
        &self,
        columns: &[Column],
        order: &[usize],
    ) -> Option<(RangeInclusive<i64>, RangeInclusive<usize>)> {
        let locate = |column| order.iter().position(|&ix| columns[ix].name == column);
        let (a, b) = (
            locate(self.anchor.column.as_str())?,
            locate(self.active.column.as_str())?,
        );
        let (first, last) = (self.anchor.row, self.active.row);
        Some((first.min(last)..=first.max(last), a.min(b)..=a.max(b)))
    }
}

pub(super) struct Grid {
    pub(super) focus: FocusHandle,
    pub(super) range: Option<Range>,
    pub(super) reveal: Option<Address>,
}

#[derive(Clone, Debug, PartialEq, gpui::Action)]
#[action(namespace = cydonia_table, no_json)]
pub(super) struct Step {
    column: i64,
    row: i64,
    extend: bool,
    tab: bool,
}

gpui::actions!(cydonia_table, [EditGridCell]);

pub(super) struct GridDrag(pub(super) EntityId);

#[derive(Clone, Copy)]
pub(super) enum Paint {
    Plain,
    Selected,
    Active { reveal: bool },
}

pub(super) fn bindings() -> Vec<KeyBinding> {
    let mut bindings = vec![KeyBinding::new("enter", EditGridCell, Some(CONTEXT))];
    for (key, shifted, column, row) in [
        ("left", "shift-left", -1, 0),
        ("right", "shift-right", 1, 0),
        ("up", "shift-up", 0, -1),
        ("down", "shift-down", 0, 1),
    ] {
        for (key, extend) in [(key, false), (shifted, true)] {
            bindings.push(KeyBinding::new(
                key,
                Step {
                    column,
                    row,
                    extend,
                    tab: false,
                },
                Some(CONTEXT),
            ));
        }
    }
    for (key, column) in [("tab", 1), ("shift-tab", -1)] {
        for context in [CONTEXT, KEY_CONTEXT] {
            bindings.push(KeyBinding::new(
                key,
                Step {
                    column,
                    row: 0,
                    extend: false,
                    tab: true,
                },
                Some(context),
            ));
        }
    }
    #[cfg(target_os = "macos")]
    let shortcuts = ("cmd-c", "cmd-v", "cmd-z");
    #[cfg(not(target_os = "macos"))]
    let shortcuts = ("ctrl-c", "ctrl-v", "ctrl-z");
    bindings.extend([
        KeyBinding::new(shortcuts.0, input::Copy, Some(CONTEXT)),
        KeyBinding::new(shortcuts.1, input::Paste, Some(CONTEXT)),
        KeyBinding::new(shortcuts.2, input::Undo, Some(CONTEXT)),
    ]);
    bindings
}

impl TableView {
    pub(super) fn select_cell(
        &mut self,
        active: Address,
        extend: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(page) = self.page(cx) else { return };
        let revision = page.revision;
        let sort = self.source.read(cx).sort.clone();
        let anchor = self
            .grid
            .range
            .as_ref()
            .filter(|range| extend && range.current(revision, sort.as_ref()))
            .map_or_else(|| active.clone(), |range| range.anchor.clone());
        self.grid.range = Some(Range {
            anchor,
            active,
            revision,
            sort,
        });
        self.clipboard.error = None;
        window.focus(&self.grid.focus, cx);
        cx.notify();
    }

    pub(super) fn step_grid(&mut self, step: &Step, window: &mut Window, cx: &mut Context<Self>) {
        let editing = if step.tab {
            match &self.cell {
                Some(Cell::Value { rowid, column }) => Some((*rowid, column.clone())),
                _ => None,
            }
        } else {
            None
        };
        if !self.grid.focus.is_focused(window) && editing.is_none() {
            cx.propagate();
            return;
        }
        if !self.commit_cell(cx) {
            return;
        }
        if editing.is_some() {
            window.focus(&self.grid.focus, cx);
        }
        let Some(page) = self.page(cx) else { return };
        let table = self.source.read(cx);
        if page.total == 0 || page.columns.is_empty() {
            return;
        }
        let order = table.record.column_layout.order(&page.columns);
        let active = self
            .grid
            .range
            .as_ref()
            .filter(|range| range.current(page.revision, table.sort.as_ref()));
        let editing = editing.and_then(|(rowid, column)| {
            let row = page.rows.iter().position(|row| row.rowid == rowid)?;
            Some((page.offset + row as i64, column))
        });
        let from = editing
            .as_ref()
            .map(|(row, column)| (*row, column.as_str()))
            .or_else(|| active.map(|range| (range.active.row, range.active.column.as_str())));
        let (row, ix) = match from {
            None => (page.offset, 0),
            Some((row, column)) => {
                let ix = order
                    .iter()
                    .position(|&ix| page.columns[ix].name == column)
                    .unwrap_or(0) as i64;
                if step.tab {
                    let columns = order.len() as i64;
                    let next = (ix + step.column).rem_euclid(columns);
                    let row = row + (ix + step.column).div_euclid(columns);
                    if !(0..page.total).contains(&row) {
                        return;
                    }
                    (row, next as usize)
                } else {
                    (
                        (row + step.row).clamp(0, page.total - 1),
                        (ix + step.column).clamp(0, order.len() as i64 - 1) as usize,
                    )
                }
            }
        };
        let column = page.columns[order[ix]].name.clone();
        let offset = row / table::PAGE * table::PAGE;
        if self.offset != offset {
            self.source
                .update(cx, |table, cx| table.load_page(offset, cx));
            if self.source.read(cx).page(offset).is_none() {
                return;
            }
            self.offset = offset;
            self.vertical
                .set_offset(gpui::point(gpui::px(0.), gpui::px(0.)));
        }
        let Some(rowid) = self
            .page(cx)
            .and_then(|page| page.rows.get((row - offset) as usize))
            .map(|row| row.rowid)
        else {
            return;
        };
        let address = Address { rowid, row, column };
        self.select_cell(address.clone(), step.extend, window, cx);
        self.grid.reveal = Some(address);
    }

    pub(super) fn edit_grid_cell(
        &mut self,
        _: &EditGridCell,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.grid.focus.is_focused(window) {
            cx.propagate();
            return;
        }
        if let Some(range) = &self.grid.range
            && let Some(page) = self.page(cx)
            && range.current(page.revision, self.source.read(cx).sort.as_ref())
        {
            self.edit_cell(
                Cell::Value {
                    rowid: range.active.rowid,
                    column: range.active.column.clone(),
                },
                window,
                cx,
            );
        }
    }

    pub(super) fn refresh_grid_after_edit(&mut self, cx: &Context<Self>) {
        let Some(range) = &self.grid.range else {
            return;
        };
        let updated = self
            .page(cx)
            .filter(|page| {
                self.source.read(cx).sort.is_none()
                    && range
                        .active
                        .row
                        .checked_sub(page.offset)
                        .and_then(|index| page.rows.get(index as usize))
                        .is_some_and(|row| row.rowid == range.active.rowid)
            })
            .map(|page| page.revision);
        match updated {
            Some(revision) => {
                if let Some(range) = &mut self.grid.range {
                    range.revision = revision;
                }
            }
            None => self.grid.range = None,
        }
    }

    pub(super) fn reveal_cell(&self, cx: &Context<Self>) -> AnyElement {
        let horizontal = self.horizontal.clone();
        let vertical = self.vertical.clone();
        let view = cx.weak_entity();
        gpui::canvas(
            move |cell, window, cx| {
                let offset = gpui::point(
                    horizontal.offset().x
                        + reveal_delta(cell, horizontal.bounds(), gpui::Axis::Horizontal),
                    vertical.offset().y
                        + reveal_delta(cell, vertical.bounds(), gpui::Axis::Vertical),
                );
                let horizontal = horizontal.clone();
                let vertical = vertical.clone();
                let view = view.clone();
                window.defer(cx, move |_, cx| {
                    horizontal.set_offset(gpui::point(offset.x, gpui::px(0.)));
                    vertical.set_offset(gpui::point(gpui::px(0.), offset.y));
                    view.update(cx, |_, cx| cx.notify()).ok();
                });
            },
            |_, _, _, _| {},
        )
        .absolute()
        .inset_0()
        .into_any_element()
    }
}

fn reveal_delta(cell: Bounds<Pixels>, view: Bounds<Pixels>, axis: gpui::Axis) -> Pixels {
    let (start, end, visible_start, visible_end) = match axis {
        gpui::Axis::Horizontal => (cell.left(), cell.right(), view.left(), view.right()),
        gpui::Axis::Vertical => (cell.top(), cell.bottom(), view.top(), view.bottom()),
    };
    if start < visible_start || end - start > visible_end - visible_start {
        visible_start - start
    } else if end > visible_end {
        visible_end - end
    } else {
        gpui::px(0.)
    }
}
