//! Notes kept in a project table: made on first run, written, read back and
//! cleared. Build it with
//! `cargo build -p cydonia-guest --example notes --target wasm32-unknown-unknown`.

use cydonia_guest::{
    App,
    node::*,
    tables::{self, Record},
    wire::{ColType, Rows, Space, Table, Tone},
};

#[derive(Default)]
struct Notes {
    table: Option<String>,
    notes: Vec<String>,
    error: Option<String>,
}

#[derive(Clone)]
enum Msg {
    Tables(Result<Vec<Table>, String>),
    Created(Result<Table, String>),
    Add,
    Written(Result<(), String>),
    Loaded(Result<Rows, String>),
    Clear,
}

impl Notes {
    fn load(&self) {
        if let Some(table) = &self.table {
            tables::query(
                format!("SELECT text FROM {table} ORDER BY rowid"),
                Msg::Loaded,
            );
        }
    }
}

impl App for Notes {
    type Msg = Msg;

    fn init() -> Self {
        tables::list(Msg::Tables);
        Notes::default()
    }

    fn update(&mut self, msg: Msg) {
        match msg {
            Msg::Tables(Ok(tables)) => match tables.into_iter().find(|t| t.name == "Notes") {
                Some(table) => {
                    self.table = Some(table.key);
                    self.load();
                }
                None => tables::create("Notes", [("text", ColType::Text)], Msg::Created),
            },
            Msg::Created(Ok(table)) => self.table = Some(table.key),
            Msg::Add => {
                if let Some(table) = &self.table {
                    let mut row = Record::new();
                    row.insert(
                        "text".into(),
                        format!("Note {}", self.notes.len() + 1).into(),
                    );
                    tables::insert(table.clone(), vec![row], |done| {
                        Msg::Written(done.map(|_| ()))
                    });
                }
            }
            Msg::Clear => {
                if let Some(table) = &self.table {
                    tables::delete(table.clone(), "text", "Note 1", Msg::Written);
                }
            }
            Msg::Written(Ok(())) => self.load(),
            Msg::Loaded(Ok(rows)) => {
                self.notes = rows
                    .rows
                    .into_iter()
                    .filter_map(|row| row.first()?.as_str().map(str::to_owned))
                    .collect();
            }
            Msg::Tables(Err(error))
            | Msg::Created(Err(error))
            | Msg::Written(Err(error))
            | Msg::Loaded(Err(error)) => self.error = Some(error),
        }
    }

    fn view(&self) -> Node<Msg> {
        column([row([
            button("Add").on_click(Msg::Add).into(),
            button("Clear").on_click(Msg::Clear).into(),
        ])
        .gap(Space::Small)
        .into()])
        .children(self.notes.iter().map(|note| text(note.clone()).into()))
        .children(
            self.error
                .iter()
                .map(|error| text(error.clone()).tone(Tone::Danger).into()),
        )
        .gap(Space::Medium)
        .padding(Space::Large)
        .into()
    }
}

cydonia_guest::app!(Notes);
