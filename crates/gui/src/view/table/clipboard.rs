use crate::{
    data::{ColType, Edit, Revision},
    view::table::{TableView, grid::Range, text},
};
use bezel::{
    gpui::{App, ClipboardItem, Context, Window},
    ui::input,
};
use serde_json::{Number, Value};

#[derive(Default)]
pub(super) struct Clipboard {
    pub(super) error: Option<String>,
    undo: Option<PasteUndo>,
}

struct PasteUndo {
    revision: Revision,
    edits: Vec<Edit>,
}

impl TableView {
    pub(super) fn copy_cells(
        &mut self,
        _: &input::Copy,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.grid.focus.is_focused(window) {
            cx.propagate();
            return;
        }
        self.clipboard.error = match self.copy_block(cx) {
            Ok(text) => {
                cx.write_to_clipboard(ClipboardItem::new_string(text));
                None
            }
            Err(error) => Some(error),
        };
        cx.notify();
    }

    fn copy_block(&self, cx: &App) -> Result<String, String> {
        let range = self.clipboard_range(cx)?;
        let table = self.source.read(cx);
        let page = self.page(cx).ok_or("The table could not be loaded")?;
        let order = table.record.column_layout.order(&page.columns);
        let (rows, columns) = range
            .limits(&page.columns, &order)
            .ok_or("The selected columns changed")?;
        let page = table.read_rows(*rows.start(), rows.end() - rows.start() + 1)?;
        if !range.current(page.revision, table.sort.as_ref()) {
            return Err("The table changed. Select the cells again.".into());
        }
        let mut writer = csv::WriterBuilder::new()
            .delimiter(b'\t')
            .from_writer(Vec::new());
        for row in &page.rows {
            let record = columns
                .clone()
                .map(|column| row.cells.get(order[column]).map(text).unwrap_or_default());
            writer
                .write_record(record)
                .map_err(|error| error.to_string())?;
        }
        let bytes = writer.into_inner().map_err(|error| error.to_string())?;
        String::from_utf8(bytes).map_err(|error| error.to_string())
    }

    pub(super) fn paste_cells(
        &mut self,
        _: &input::Paste,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.grid.focus.is_focused(window) {
            cx.propagate();
            return;
        }
        let Some(raw) = cx.read_from_clipboard().and_then(|item| item.text()) else {
            return;
        };
        self.clipboard.error = self.paste_block(&raw, cx).err();
        cx.notify();
    }

    fn paste_block(&mut self, raw: &str, cx: &mut Context<Self>) -> Result<(), String> {
        let range = self.clipboard_range(cx)?.clone();
        let table = self.source.read(cx);
        let page = self.page(cx).ok_or("The table could not be loaded")?;
        let layout = &table.record.column_layout;
        let order = layout.order(&page.columns);
        let column = order
            .iter()
            .position(|&ix| page.columns[ix].name == range.active.column)
            .ok_or("The selected column changed")?;
        let (records, width) = read_block(raw)?;
        if records.is_empty() {
            return Ok(());
        }
        if width > order.len() - column || records.len() as i64 > page.total - range.active.row {
            return Err("The copied cells do not fit in the table.".into());
        }
        let targets = table.read_rows(range.active.row, records.len() as i64)?;
        if !range.current(targets.revision, table.sort.as_ref())
            || targets.rows.len() != records.len()
            || targets.rows.first().map(|row| row.rowid) != Some(range.active.rowid)
        {
            return Err("The table changed. Select the starting cell again.".into());
        }
        let mut edits = Vec::new();
        let mut undo = Vec::new();
        for (row_index, (record, target)) in records.iter().zip(&targets.rows).enumerate() {
            for (field, &ix) in order[column..column + width].iter().enumerate() {
                let input = record
                    .as_ref()
                    .and_then(|record| record.get(field))
                    .unwrap_or_default();
                let column = &targets.columns[ix];
                let options = layout
                    .settings
                    .get(&column.name)
                    .map(|settings| settings.select_options.as_slice())
                    .unwrap_or_default();
                let Some(value) = paste_value(input, column.kind, options) else {
                    let row_number = range.active.row + row_index as i64 + 1;
                    return Err(format!(
                        "Invalid {} value for {} in row {row_number}.",
                        column.kind.name(),
                        column.name
                    ));
                };
                let old = &target.cells[ix];
                if *old != value {
                    edits.push(Edit {
                        rowid: target.rowid,
                        column: column.name.clone(),
                        value,
                    });
                    undo.push(Edit {
                        rowid: target.rowid,
                        column: column.name.clone(),
                        value: old.clone(),
                    });
                }
            }
        }
        if edits.is_empty() {
            return Ok(());
        }
        self.write_clipboard_batch(&edits, targets.revision, cx)?;
        self.clipboard.undo = self.page(cx).map(|page| PasteUndo {
            revision: page.revision,
            edits: undo,
        });
        Ok(())
    }

    pub(super) fn undo_paste(
        &mut self,
        _: &input::Undo,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.grid.focus.is_focused(window) {
            cx.propagate();
            return;
        }
        let Some(undo) = self.clipboard.undo.take() else {
            return;
        };
        self.clipboard.error = self
            .write_clipboard_batch(&undo.edits, undo.revision, cx)
            .err();
        cx.notify();
    }

    fn clipboard_range(&self, cx: &App) -> Result<&Range, String> {
        let page = self.page(cx).ok_or("The table could not be loaded")?;
        let table = self.source.read(cx);
        let range = self.grid.range.as_ref().ok_or("Select a cell first.")?;
        if !range.current(page.revision, table.sort.as_ref()) {
            return Err("The table changed. Select the cells again.".into());
        }
        Ok(range)
    }

    fn write_clipboard_batch(
        &mut self,
        edits: &[Edit],
        revision: Revision,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let entry = self.table.clone();
        self.workspace.update(cx, |workspace, cx| {
            workspace.edit_table(&entry, cx, |table, cx| {
                table.write_cells(edits, Some(revision), cx)
            })
        })?;
        self.refresh_grid_after_edit(cx);
        Ok(())
    }
}

fn read_block(raw: &str) -> Result<(Vec<Option<csv::StringRecord>>, usize), String> {
    let bytes = raw.as_bytes();
    let mut reader = csv::ReaderBuilder::new()
        .delimiter(b'\t')
        .has_headers(false)
        .from_reader(bytes);
    let mut records = Vec::new();
    let mut width = 1;
    loop {
        let start = reader.position().byte() as usize;
        let mut remaining = &bytes[start..];
        if start > 0 && bytes[start - 1] == b'\r' {
            remaining = remaining.strip_prefix(b"\n").unwrap_or(remaining);
        }
        // The CSV reader skips empty records. A blank clipboard row clears its cells.
        while let Some(after) = remaining
            .strip_prefix(b"\r\n")
            .or_else(|| remaining.strip_prefix(b"\n"))
            .or_else(|| remaining.strip_prefix(b"\r"))
        {
            records.push(None);
            remaining = after;
        }
        let mut record = csv::StringRecord::new();
        if !reader
            .read_record(&mut record)
            .map_err(|error| format!("Could not paste the copied cells. {error}"))?
        {
            break;
        }
        width = record.len();
        records.push(Some(record));
    }
    Ok((records, width))
}

fn paste_value(raw: &str, kind: ColType, options: &[String]) -> Option<Value> {
    if raw.is_empty() {
        return Some(Value::Null);
    }
    let trimmed = raw.trim();
    match kind {
        ColType::Text | ColType::Url => Some(Value::String(raw.to_owned())),
        ColType::Select => options
            .iter()
            .any(|option| option == raw)
            .then(|| Value::String(raw.to_owned())),
        ColType::Number => trimmed
            .parse::<i64>()
            .ok()
            .map(Number::from)
            .or_else(|| trimmed.parse::<f64>().ok().and_then(Number::from_f64))
            .map(Value::Number),
        ColType::Date => trimmed
            .parse::<i64>()
            .ok()
            .or_else(|| {
                chrono::NaiveDate::parse_from_str(trimmed, "%Y-%m-%d")
                    .ok()
                    .and_then(|date| date.and_hms_opt(0, 0, 0))
                    .map(|date| date.and_utc().timestamp())
            })
            .map(Value::from),
        ColType::Check if trimmed == "1" || trimmed.eq_ignore_ascii_case("true") => {
            Some(Value::Bool(true))
        }
        ColType::Check if trimmed == "0" || trimmed.eq_ignore_ascii_case("false") => {
            Some(Value::Bool(false))
        }
        ColType::Check => None,
    }
}
