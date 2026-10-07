//! Activity and token usage across the open projects. Activity is read from
//! each project's files; usage from the logs Claude Code and Codex keep of
//! every session run in the project's directory, whoever started it, through
//! the project's index of them — see [`index`]. Costs are not here: they need
//! the price list, and are worked out when drawn.

mod claude;
mod codex;
mod index;

use anyhow::Result;
use artifact::project::{Project as _, fs};
use chrono::{Duration, NaiveDate};
use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
};

/// The ranges spend is shown over, in days.
pub const RANGES: [u32; 3] = [7, 30, 90];

/// How many days the heatmap covers: 53 weeks.
pub const HEAT_DAYS: u32 = 53 * 7;

/// Token counts, as one model request or a sum of them.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Tokens {
    /// Input read uncached.
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

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Day {
    pub date: String,
    /// Sessions a message was sent in that day.
    pub sessions: u64,
    pub articles: u64,
    pub cards: u64,
}

impl Day {
    pub fn total(&self) -> u64 {
        self.sessions + self.articles + self.cards
    }
}

/// One model's tokens on one day.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Spend {
    pub day: String,
    pub model: String,
    pub tokens: Tokens,
}

#[derive(Clone, Debug, Default)]
pub struct Summary {
    /// The range's dates, oldest first, today last.
    pub days: Vec<String>,
    /// [`HEAT_DAYS`] of activity, oldest first, today last.
    pub heat: Vec<Day>,
    /// The range's usage, a row per day and model.
    pub spend: Vec<Spend>,
}

/// Where the agents keep their logs.
pub struct Logs {
    /// Claude Code's `projects` directory.
    pub claude: Option<PathBuf>,
    /// Codex's `sessions` directory.
    pub codex: Option<PathBuf>,
}

impl Logs {
    /// `$CLAUDE_CONFIG_DIR` and `$CODEX_HOME`, else the agents' defaults
    /// under the home directory.
    pub fn local() -> Self {
        let under = |var: &str, default: &str| {
            std::env::var_os(var)
                .map(PathBuf::from)
                .or_else(|| dirs::home_dir().map(|home| home.join(default)))
        };
        Self {
            claude: under("CLAUDE_CONFIG_DIR", ".claude").map(|dir| dir.join("projects")),
            codex: under("CODEX_HOME", ".codex").map(|dir| dir.join("sessions")),
        }
    }
}

impl Summary {
    /// Blocking: reads every session, article and board in `projects`, and
    /// what the agents' logs have had appended since each project's index last
    /// read them.
    pub fn compute(projects: &[PathBuf], logs: &Logs, days: u32, today: NaiveDate) -> Result<Self> {
        let key = |date: NaiveDate| date.format("%Y-%m-%d").to_string();
        let dates = |count: u32| -> Vec<String> {
            (0..count)
                .rev()
                .map(|back| key(today - Duration::days(i64::from(back))))
                .collect()
        };
        let range = days;
        let mut heat: Vec<Day> = dates(HEAT_DAYS)
            .into_iter()
            .map(|date| Day {
                date,
                ..Day::default()
            })
            .collect();
        let index: HashMap<String, usize> = heat
            .iter()
            .enumerate()
            .map(|(ix, day)| (day.date.clone(), ix))
            .collect();
        let at = |ms: i64| local_day(ms / 1000).and_then(|day| index.get(&day).copied());

        for project in projects {
            let store = fs::Project::new(project);
            for record in store.sessions() {
                let worked: HashSet<usize> = record
                    .sent_at
                    .values()
                    .filter_map(|sent| i64::try_from(*sent).ok())
                    .filter_map(|secs| at(secs * 1000))
                    .collect();
                for ix in worked {
                    heat[ix].sessions += 1;
                }
            }
            // Article and card ids are the millisecond they were minted, `-2`
            // and on for a tie.
            for article in store.articles() {
                if let Some(ix) = minted(&article.id).and_then(at) {
                    heat[ix].articles += 1;
                }
            }
            for card in store
                .boards()
                .iter()
                .flat_map(|board| &board.columns)
                .flat_map(|column| &column.cards)
            {
                if let Some(ix) = minted(&card.id).and_then(at) {
                    heat[ix].cards += 1;
                }
            }
        }

        for project in projects {
            index::Index::open(project)?.refresh(logs)?;
        }
        let (days, spend) = Self::spend(projects, range, today)?;
        Ok(Self { days, heat, spend })
    }

    /// The range's dates and usage, from the projects' indexes as they stand.
    /// Blocking, but reads no log.
    pub fn spend(
        projects: &[PathBuf],
        days: u32,
        today: NaiveDate,
    ) -> Result<(Vec<String>, Vec<Spend>)> {
        let days: Vec<String> = (0..days.max(1))
            .rev()
            .map(|back| {
                (today - Duration::days(i64::from(back)))
                    .format("%Y-%m-%d")
                    .to_string()
            })
            .collect();
        let mut by: HashMap<(String, String), Tokens> = HashMap::new();
        for project in projects {
            for (day, model, tokens) in index::Index::open(project)?.spend(&days[0])? {
                *by.entry((day, model)).or_default() += tokens;
            }
        }
        let mut spend: Vec<Spend> = by
            .into_iter()
            .map(|((day, model), tokens)| Spend { day, model, tokens })
            .collect();
        spend.sort_by(|a, b| (&a.day, &a.model).cmp(&(&b.day, &b.model)));
        Ok((days, spend))
    }
}

/// One model request, as an agent's log has it.
struct Request {
    at: chrono::DateTime<chrono::Utc>,
    model: String,
    tokens: Tokens,
}

/// A path as compared with the directory an agent logs: symlinks resolved.
fn settled(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}

/// The millisecond an id was minted at.
fn minted(id: &str) -> Option<i64> {
    id.split('-').next()?.parse().ok()
}

/// The local date of a Unix time in seconds, as a day is keyed.
fn local_day(secs: i64) -> Option<String> {
    use chrono::TimeZone as _;
    chrono::Local
        .timestamp_opt(secs, 0)
        .single()
        .map(|at| at.format("%Y-%m-%d").to_string())
}
