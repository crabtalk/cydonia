//! A project's activity by day, in `state.db` beside the entry numbers.
//!
//! Every write adds into its day's row; nothing is overwritten. A day is the
//! caller's local date as `YYYY-MM-DD`, so ranges compare as text.

use crate::project::fs;
use anyhow::Result;
use rusqlite::{Connection, params};
use std::{path::Path, time::Duration};

const DDL: &str = "
CREATE TABLE IF NOT EXISTS session_usage (
    day TEXT NOT NULL,
    session TEXT NOT NULL,
    model TEXT NOT NULL,
    input INTEGER NOT NULL DEFAULT 0,
    output INTEGER NOT NULL DEFAULT 0,
    cache_read INTEGER NOT NULL DEFAULT 0,
    cache_write INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (day, session, model)
);
CREATE TABLE IF NOT EXISTS session_messages (
    day TEXT NOT NULL,
    session TEXT NOT NULL,
    count INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (day, session)
);
CREATE TABLE IF NOT EXISTS article_words (
    day TEXT NOT NULL,
    article TEXT NOT NULL,
    added INTEGER NOT NULL DEFAULT 0,
    removed INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (day, article)
);
CREATE TABLE IF NOT EXISTS card_events (
    day TEXT NOT NULL,
    card TEXT NOT NULL,
    created INTEGER NOT NULL DEFAULT 0,
    done INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (day, card)
);
";

/// Token counts as an agent reports them for one prompt turn.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Tokens {
    pub input: u64,
    pub output: u64,
    pub cache_read: u64,
    pub cache_write: u64,
}

impl std::ops::AddAssign for Tokens {
    fn add_assign(&mut self, other: Self) {
        self.input += other.input;
        self.output += other.output;
        self.cache_read += other.cache_read;
        self.cache_write += other.cache_write;
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Usage {
    pub day: String,
    pub session: String,
    pub model: String,
    pub tokens: Tokens,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Messages {
    pub day: String,
    pub session: String,
    pub count: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Words {
    pub day: String,
    pub article: String,
    pub added: u64,
    pub removed: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Cards {
    pub day: String,
    pub card: String,
    pub created: u64,
    pub done: u64,
}

/// Every row of a range, `from` and `to` inclusive.
#[derive(Clone, Debug, Default)]
pub struct Range {
    pub usage: Vec<Usage>,
    pub messages: Vec<Messages>,
    pub words: Vec<Words>,
    pub cards: Vec<Cards>,
}

pub struct Stats(Connection);

impl Stats {
    pub fn open(project: &Path) -> Result<Self> {
        let connection = Connection::open(fs::Project::new(project).state()?)?;
        connection.busy_timeout(Duration::from_secs(5))?;
        connection.execute_batch(DDL)?;
        Ok(Self(connection))
    }

    pub fn usage(&self, day: &str, session: &str, model: &str, tokens: Tokens) -> Result<()> {
        self.0.execute(
            "INSERT INTO session_usage (day, session, model, input, output, cache_read, cache_write)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT DO UPDATE SET input = input + ?4, output = output + ?5,
                 cache_read = cache_read + ?6, cache_write = cache_write + ?7",
            params![
                day,
                session,
                model,
                tokens.input as i64,
                tokens.output as i64,
                tokens.cache_read as i64,
                tokens.cache_write as i64
            ],
        )?;
        Ok(())
    }

    pub fn message(&self, day: &str, session: &str) -> Result<()> {
        self.0.execute(
            "INSERT INTO session_messages (day, session, count) VALUES (?1, ?2, 1)
             ON CONFLICT DO UPDATE SET count = count + 1",
            params![day, session],
        )?;
        Ok(())
    }

    pub fn words(&self, day: &str, article: &str, added: u64, removed: u64) -> Result<()> {
        if added == 0 && removed == 0 {
            return Ok(());
        }
        self.0.execute(
            "INSERT INTO article_words (day, article, added, removed) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT DO UPDATE SET added = added + ?3, removed = removed + ?4",
            params![day, article, added as i64, removed as i64],
        )?;
        Ok(())
    }

    pub fn card_created(&self, day: &str, card: &str) -> Result<()> {
        self.card(day, card, 1, 0)
    }

    pub fn card_done(&self, day: &str, card: &str) -> Result<()> {
        self.card(day, card, 0, 1)
    }

    fn card(&self, day: &str, card: &str, created: i64, done: i64) -> Result<()> {
        self.0.execute(
            "INSERT INTO card_events (day, card, created, done) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT DO UPDATE SET created = created + ?3, done = done + ?4",
            params![day, card, created, done],
        )?;
        Ok(())
    }

    pub fn range(&self, from: &str, to: &str) -> Result<Range> {
        let rows = |sql: &str| -> Result<Vec<Vec<rusqlite::types::Value>>> {
            let mut stmt = self.0.prepare(sql)?;
            let count = stmt.column_count();
            let rows = stmt
                .query_map([from, to], |row| {
                    (0..count)
                        .map(|ix| row.get(ix))
                        .collect::<rusqlite::Result<_>>()
                })?
                .collect::<rusqlite::Result<_>>()?;
            Ok(rows)
        };
        use rusqlite::types::Value as V;
        let text = |value: &V| match value {
            V::Text(text) => text.clone(),
            _ => String::new(),
        };
        let int = |value: &V| match value {
            V::Integer(n) => (*n).max(0) as u64,
            _ => 0,
        };
        let range = "WHERE day >= ?1 AND day <= ?2 ORDER BY day";
        Ok(Range {
            usage: rows(&format!(
                "SELECT day, session, model, input, output, cache_read, cache_write FROM session_usage {range}"
            ))?
            .iter()
            .map(|row| Usage {
                day: text(&row[0]),
                session: text(&row[1]),
                model: text(&row[2]),
                tokens: Tokens {
                    input: int(&row[3]),
                    output: int(&row[4]),
                    cache_read: int(&row[5]),
                    cache_write: int(&row[6]),
                },
            })
            .collect(),
            messages: rows(&format!(
                "SELECT day, session, count FROM session_messages {range}"
            ))?
            .iter()
            .map(|row| Messages {
                day: text(&row[0]),
                session: text(&row[1]),
                count: int(&row[2]),
            })
            .collect(),
            words: rows(&format!(
                "SELECT day, article, added, removed FROM article_words {range}"
            ))?
            .iter()
            .map(|row| Words {
                day: text(&row[0]),
                article: text(&row[1]),
                added: int(&row[2]),
                removed: int(&row[3]),
            })
            .collect(),
            cards: rows(&format!(
                "SELECT day, card, created, done FROM card_events {range}"
            ))?
            .iter()
            .map(|row| Cards {
                day: text(&row[0]),
                card: text(&row[1]),
                created: int(&row[2]),
                done: int(&row[3]),
            })
            .collect(),
        })
    }
}

/// The local date, as a day is keyed.
pub fn today() -> String {
    chrono::Local::now().format("%Y-%m-%d").to_string()
}

/// Words added and removed going from `old` to `new`, as a multiset difference
/// of whitespace-separated words: reordering counts nothing.
pub fn word_delta(old: &str, new: &str) -> (u64, u64) {
    let mut counts = std::collections::HashMap::<&str, i64>::new();
    for word in new.split_whitespace() {
        *counts.entry(word).or_default() += 1;
    }
    for word in old.split_whitespace() {
        *counts.entry(word).or_default() -= 1;
    }
    counts.values().fold((0, 0), |(added, removed), &n| {
        if n > 0 {
            (added + n as u64, removed)
        } else {
            (added, removed + n.unsigned_abs())
        }
    })
}
