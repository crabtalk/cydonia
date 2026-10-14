//! The `companies` and `founders` project tables: their columns, and a
//! company or a founder as a row of them.

use crate::scrape::{Company, Founder};
use cydonia_guest::{tables::Record, wire::ColType};
use serde_json::Value;

/// The tables' names. Each one's key, what SQL names it by, is its name in
/// lowercase.
pub const COMPANIES: &str = "Companies";
pub const FOUNDERS: &str = "Founders";

pub const COMPANY_COLUMNS: [(&str, ColType); 16] = [
    ("slug", ColType::Text),
    ("name", ColType::Text),
    ("batch", ColType::Text),
    ("batch_name", ColType::Text),
    ("one_liner", ColType::Text),
    ("website", ColType::Text),
    ("url", ColType::Text),
    ("logo", ColType::Text),
    ("status", ColType::Text),
    ("team_size", ColType::Number),
    ("industry", ColType::Text),
    ("tags", ColType::Text),
    ("city", ColType::Text),
    ("country", ColType::Text),
    ("year_founded", ColType::Number),
    ("partner", ColType::Text),
];

pub const FOUNDER_COLUMNS: [(&str, ColType); 8] = [
    ("company", ColType::Text),
    ("batch", ColType::Text),
    ("name", ColType::Text),
    ("title", ColType::Text),
    ("bio", ColType::Text),
    ("linkedin", ColType::Text),
    ("x", ColType::Text),
    ("avatar", ColType::Text),
];

/// A table's key from its name.
pub fn key(name: &str) -> String {
    name.to_lowercase()
}

/// Text as a SQL string literal.
pub fn literal(text: &str) -> String {
    format!("'{}'", text.replace('\'', "''"))
}

/// `SELECT` every column of `columns` from `table` where `filter` holds.
pub fn query<const N: usize>(columns: &[(&str, ColType); N], table: &str, filter: &str) -> String {
    let names: Vec<&str> = columns.iter().map(|(name, _)| *name).collect();
    format!(
        "SELECT {} FROM {} WHERE {filter} ORDER BY rowid",
        names.join(", "),
        key(table)
    )
}

impl Company {
    pub fn record(&self) -> Record {
        let values: [Value; 16] = [
            self.slug.clone().into(),
            self.name.clone().into(),
            self.batch.clone().into(),
            self.batch_name.clone().into(),
            self.one_liner.clone().into(),
            self.website.clone().into(),
            self.url.clone().into(),
            self.logo.clone().into(),
            self.status.clone().into(),
            self.team_size.into(),
            self.industry.clone().into(),
            self.tags.clone().into(),
            self.city.clone().into(),
            self.country.clone().into(),
            self.year_founded.into(),
            self.partner.clone().into(),
        ];
        record(&COMPANY_COLUMNS, values)
    }

    /// A row read with [`query`] over [`COMPANY_COLUMNS`].
    pub fn from_row(row: &[Value]) -> Company {
        let text = |ix: usize| cell(row, ix);
        Company {
            slug: text(0),
            name: text(1),
            batch: text(2),
            batch_name: text(3),
            one_liner: text(4),
            website: text(5),
            url: text(6),
            logo: text(7),
            status: text(8),
            team_size: row.get(9).and_then(Value::as_i64),
            industry: text(10),
            tags: text(11),
            city: text(12),
            country: text(13),
            year_founded: row.get(14).and_then(Value::as_i64),
            partner: text(15),
        }
    }
}

impl Founder {
    pub fn record(&self) -> Record {
        let values: [Value; 8] = [
            self.company.clone().into(),
            self.batch.clone().into(),
            self.name.clone().into(),
            self.title.clone().into(),
            self.bio.clone().into(),
            self.linkedin.clone().into(),
            self.x.clone().into(),
            self.avatar.clone().into(),
        ];
        record(&FOUNDER_COLUMNS, values)
    }

    /// A row read with [`query`] over [`FOUNDER_COLUMNS`].
    pub fn from_row(row: &[Value]) -> Founder {
        let text = |ix: usize| cell(row, ix);
        Founder {
            company: text(0),
            batch: text(1),
            name: text(2),
            title: text(3),
            bio: text(4),
            linkedin: text(5),
            x: text(6),
            avatar: text(7),
        }
    }
}

fn record<const N: usize>(columns: &[(&str, ColType); N], values: [Value; N]) -> Record {
    columns
        .iter()
        .zip(values)
        .map(|((name, _), value)| ((*name).to_owned(), value))
        .collect()
}

fn cell(row: &[Value], ix: usize) -> String {
    match row.get(ix) {
        Some(Value::String(text)) => text.clone(),
        Some(Value::Number(number)) => number.to_string(),
        _ => String::new(),
    }
}
