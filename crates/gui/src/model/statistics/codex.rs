//! Codex's logs: `sessions/<year>/<month>/<day>/rollout-*.jsonl`, one per
//! thread, every project's together. The first line names the working
//! directory, each `turn_context` the model the turn runs on, and each
//! `token_count` event the thread's running total — a request is the
//! difference between two totals. `input_tokens` there includes cached input.

use super::{Request, Tokens, settled};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::{Path, PathBuf};

/// Where a thread's log has been read to, kept between reads.
#[derive(Default, Serialize, Deserialize)]
pub(super) struct Thread {
    /// Whether its working directory is the project being indexed; `None`
    /// before the first line is read.
    mine: Option<bool>,
    model: Option<String>,
    last: Total,
}

impl Thread {
    pub(super) fn restore(state: &str) -> Self {
        serde_json::from_str(state).unwrap_or_default()
    }

    pub(super) fn save(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }

    /// Take in one line of the log; the request it records, if any.
    pub(super) fn step(&mut self, line: &str, project: &Path) -> Option<Request> {
        if self.mine == Some(false) {
            return None;
        }
        let value: Value = serde_json::from_str(line).ok()?;
        let payload = value.get("payload")?;
        if self.mine.is_none() {
            let cwd = payload
                .get("cwd")
                .and_then(Value::as_str)
                .map(PathBuf::from);
            self.mine = Some(cwd.is_some_and(|cwd| settled(&cwd) == project));
            return None;
        }
        match value.get("type")?.as_str()? {
            "turn_context" => {
                self.model = payload
                    .get("model")
                    .and_then(Value::as_str)
                    .map(str::to_owned);
                None
            }
            "event_msg" if payload.get("type")?.as_str()? == "token_count" => {
                let total = Total::of(payload.get("info")?.get("total_token_usage")?);
                // A total that went down started over.
                let delta = match total.input >= self.last.input {
                    true => total.minus(&self.last),
                    false => total,
                };
                self.last = total;
                if delta.input + delta.output == 0 {
                    return None;
                }
                Some(Request {
                    at: value.get("timestamp")?.as_str()?.parse().ok()?,
                    model: self.model.clone()?,
                    tokens: Tokens {
                        input: delta.input.saturating_sub(delta.cached),
                        output: delta.output,
                        cache_read: delta.cached,
                        cache_write: 0,
                    },
                })
            }
            _ => None,
        }
    }
}

/// A thread's running total.
#[derive(Clone, Copy, Default, Serialize, Deserialize)]
struct Total {
    input: u64,
    cached: u64,
    output: u64,
}

impl Total {
    fn of(value: &Value) -> Self {
        let field = |name: &str| value.get(name).and_then(Value::as_u64).unwrap_or(0);
        Self {
            input: field("input_tokens"),
            cached: field("cached_input_tokens"),
            output: field("output_tokens"),
        }
    }

    fn minus(&self, other: &Self) -> Self {
        Self {
            input: self.input.saturating_sub(other.input),
            cached: self.cached.saturating_sub(other.cached),
            output: self.output.saturating_sub(other.output),
        }
    }
}
