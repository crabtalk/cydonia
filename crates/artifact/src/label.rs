//! Labels: names an entry carries, several at once, shared across projects.
//!
//! A label exists while at least one entry carries it. Stored as
//! `labels = ["research", "q3"]` on the entry.
//!
//! A project's [`FILE`] holds the labels registered there, a table each by
//! name, with what a label has beside the entries carrying it:
//!
//! ```toml
//! [bug]
//! color = "red"
//! description = "Something is broken"
//!
//! [draft]
//! ```
//!
//! A table can stand with nothing in it, and with no entry carrying its label.
//! Edited in place with `toml_edit`, so keys and comments this crate does not
//! know about are kept.

use std::collections::BTreeMap;
use toml_edit::{DocumentMut, Item, Table, value};

/// What the file is called, in a project's `.cydonia/`.
pub const FILE: &str = "labels.toml";

const COLOR: &str = "color";
const DESCRIPTION: &str = "description";

/// What one table says.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Label {
    /// As written: a preset's name or `#rrggbb`.
    pub color: Option<String>,
    pub description: Option<String>,
}

/// `text` as a label: lowercase and trimmed, with each run of whitespace as
/// one `-`. `None` for text with nothing in it.
pub fn normalize(text: &str) -> Option<String> {
    let name = text
        .split_whitespace()
        .collect::<Vec<_>>()
        .join("-")
        .to_lowercase();
    (!name.is_empty()).then_some(name)
}

/// Every name in `names` normalised, sorted, each once.
pub fn normalize_all<S: AsRef<str>>(names: impl IntoIterator<Item = S>) -> Vec<String> {
    let mut labels: Vec<String> = names
        .into_iter()
        .filter_map(|name| normalize(name.as_ref()))
        .collect();
    labels.sort();
    labels.dedup();
    labels
}

/// Every table in [`FILE`]'s text, by name. Text that does not parse holds
/// none.
pub fn read(text: &str) -> BTreeMap<String, Label> {
    let Ok(doc) = text.parse::<DocumentMut>() else {
        return BTreeMap::new();
    };
    doc.iter()
        .filter_map(|(name, item)| {
            let table = item.as_table_like()?;
            let field = |key: &str| {
                table
                    .get(key)
                    .and_then(|item| item.as_str())
                    .filter(|text| !text.is_empty())
                    .map(str::to_owned)
            };
            Some((
                name.to_owned(),
                Label {
                    color: field(COLOR),
                    description: field(DESCRIPTION),
                },
            ))
        })
        .collect()
}

/// `text` with `name`'s table saying what `label` says, made where there is
/// none. A field `label` leaves empty is taken out. Text that does not parse
/// comes back as it was.
pub fn save(text: &str, name: &str, label: &Label) -> String {
    edit(text, |doc| {
        if !doc.contains_table(name) {
            doc.insert(name, Item::Table(Table::new()));
        }
        let Some(table) = doc.get_mut(name).and_then(Item::as_table_mut) else {
            return;
        };
        for (key, field) in [(COLOR, &label.color), (DESCRIPTION, &label.description)] {
            match field {
                Some(field) => {
                    table.insert(key, value(field.as_str()));
                }
                None => {
                    table.remove(key);
                }
            }
        }
    })
}

/// `text` with `from`'s table moved to `to`. Onto a label that has a table,
/// `to`'s own keys are kept and `from`'s fill the rest.
pub fn rename(text: &str, from: &str, to: &str) -> String {
    if from == to {
        return text.to_owned();
    }
    edit(text, |doc| {
        let Some(Item::Table(moved)) = doc.remove(from) else {
            return;
        };
        match doc.get_mut(to).and_then(Item::as_table_mut) {
            Some(held) => {
                for (key, item) in moved.iter() {
                    if !held.contains_key(key) {
                        held.insert(key, item.clone());
                    }
                }
            }
            None => {
                doc.insert(to, Item::Table(moved));
            }
        }
    })
}

/// `text` without `name`'s table.
pub fn remove(text: &str, name: &str) -> String {
    edit(text, |doc| {
        doc.remove(name);
    })
}

fn edit(text: &str, change: impl FnOnce(&mut DocumentMut)) -> String {
    let Ok(mut doc) = text.parse::<DocumentMut>() else {
        return text.to_owned();
    };
    change(&mut doc);
    doc.to_string()
}
