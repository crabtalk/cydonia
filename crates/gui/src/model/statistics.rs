//! One project's statistics over a range of days, read from `state.db` and
//! the project's files. Costs are not here: they need the price list, and are
//! worked out when drawn.

use anyhow::Result;
use artifact::{
    project::{Project as _, fs},
    stats::{Stats, Tokens},
};
use chrono::{Duration, NaiveDate};
use std::{collections::HashMap, path::Path};

/// The ranges the page offers, in days.
pub const RANGES: [u32; 3] = [7, 30, 90];

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Day {
    pub date: String,
    pub words_added: u64,
    pub words_removed: u64,
    pub cards_created: u64,
    pub cards_done: u64,
    pub messages: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Spend {
    pub day: String,
    pub session: String,
    /// The agent the session was filed under, `unknown` for a session no
    /// longer on disk.
    pub agent: String,
    pub model: String,
    pub tokens: Tokens,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CardLink {
    pub board: String,
    pub card: String,
    pub session: String,
}

#[derive(Clone, Debug, Default)]
pub struct Summary {
    /// Oldest first, one per day of the range, today last.
    pub days: Vec<Day>,
    /// Words in every article now, archived ones included.
    pub words_total: u64,
    /// Title and net words written in range, most first, at most five.
    pub grew: Vec<(String, i64)>,
    /// Each board not archived, with its columns' card counts.
    pub columns: Vec<(String, Vec<(String, usize)>)>,
    pub spend: Vec<Spend>,
    /// Cards linked to a session, for cost per card and per board.
    pub links: Vec<CardLink>,
}

impl Summary {
    /// Blocking: opens `state.db` and reads every article, board and session
    /// record in the project.
    pub fn compute(project: &Path, days: u32, today: NaiveDate) -> Result<Self> {
        let first = today - Duration::days(i64::from(days.max(1)) - 1);
        let key = |date: NaiveDate| date.format("%Y-%m-%d").to_string();
        let range = Stats::open(project)?.range(&key(first), &key(today))?;
        let store = fs::Project::new(project);

        let mut by_day: Vec<Day> = (0..days.max(1))
            .map(|offset| Day {
                date: key(first + Duration::days(i64::from(offset))),
                ..Day::default()
            })
            .collect();
        let index: HashMap<String, usize> = by_day
            .iter()
            .enumerate()
            .map(|(ix, day)| (day.date.clone(), ix))
            .collect();
        let mut net: HashMap<String, i64> = HashMap::new();
        for row in &range.words {
            if let Some(&ix) = index.get(&row.day) {
                by_day[ix].words_added += row.added;
                by_day[ix].words_removed += row.removed;
            }
            *net.entry(row.article.clone()).or_default() += row.added as i64 - row.removed as i64;
        }
        for row in &range.cards {
            if let Some(&ix) = index.get(&row.day) {
                by_day[ix].cards_created += row.created;
                by_day[ix].cards_done += row.done;
            }
        }
        for row in &range.messages {
            if let Some(&ix) = index.get(&row.day) {
                by_day[ix].messages += row.count;
            }
        }

        let articles = store.articles();
        let words_total = articles
            .iter()
            .filter_map(|article| store.read_article(&article.id).ok())
            .map(|body| body.split_whitespace().count() as u64)
            .sum();
        let titles: HashMap<&str, &str> = articles
            .iter()
            .map(|article| (article.id.as_str(), article.title.as_str()))
            .collect();
        let mut grew: Vec<(String, i64)> = net
            .into_iter()
            .filter(|(_, words)| *words > 0)
            .map(|(id, words)| {
                let title = titles
                    .get(id.as_str())
                    .map_or(id.clone(), |t| (*t).to_owned());
                (title, words)
            })
            .collect();
        grew.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        grew.truncate(5);

        let boards = store.boards();
        let columns = boards
            .iter()
            .filter(|board| !board.archived)
            .map(|board| {
                (
                    board.label().to_owned(),
                    board
                        .columns
                        .iter()
                        .map(|column| (column.name.clone(), column.cards.len()))
                        .collect(),
                )
            })
            .collect();
        let links = boards
            .iter()
            .flat_map(|board| {
                board.columns.iter().flat_map(move |column| {
                    column.cards.iter().filter_map(move |card| {
                        Some(CardLink {
                            board: board.label().to_owned(),
                            card: match board.handle_of(card) {
                                Some(handle) => format!("{handle} {}", first_line(&card.text)),
                                None => first_line(&card.text).to_owned(),
                            },
                            session: card.session.clone()?,
                        })
                    })
                })
            })
            .collect();

        let agents: HashMap<String, String> = match range.usage.is_empty() {
            true => HashMap::new(),
            false => store
                .sessions()
                .into_iter()
                .map(|record| (record.id, record.agent))
                .collect(),
        };
        let spend = range
            .usage
            .into_iter()
            .map(|row| Spend {
                agent: agents
                    .get(&row.session)
                    .cloned()
                    .unwrap_or_else(|| "unknown".to_owned()),
                day: row.day,
                session: row.session,
                model: row.model,
                tokens: row.tokens,
            })
            .collect();

        Ok(Self {
            days: by_day,
            words_total,
            grew,
            columns,
            spend,
            links,
        })
    }
}

fn first_line(text: &str) -> &str {
    text.lines().next().unwrap_or_default().trim()
}
