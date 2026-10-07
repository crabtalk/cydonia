//! Claude Code's logs: `projects/<directory>/<session>.jsonl`, subagents in
//! folders under it. Each assistant message carries its model and the usage
//! of the request that produced it, repeated on every line the message is
//! split across and again in a resumed session's copy of the history.

use super::{Request, Tokens};
use serde_json::Value;
use std::path::Path;

/// What Claude Code files placeholder turns under; no request was made.
const SYNTHETIC: &str = "<synthetic>";

/// Claude Code's folder for a working directory: every character that is not
/// a letter or a digit becomes `-`.
pub(super) fn folder_name(path: &Path) -> String {
    path.to_string_lossy()
        .chars()
        .map(|ch| match ch.is_ascii_alphanumeric() {
            true => ch,
            false => '-',
        })
        .collect()
}

/// The request an assistant message line records, by its message and request
/// ids.
pub(super) fn parse(line: &str) -> Option<(String, Request)> {
    // Most lines are not assistant messages; skip them before parsing.
    if !line.contains("\"usage\"") {
        return None;
    }
    let value: Value = serde_json::from_str(line).ok()?;
    if value.get("type")?.as_str()? != "assistant" {
        return None;
    }
    let message = value.get("message")?;
    let model = message.get("model")?.as_str()?;
    if model == SYNTHETIC {
        return None;
    }
    let usage = message.get("usage")?;
    let field = |name: &str| usage.get(name).and_then(Value::as_u64).unwrap_or(0);
    let id = format!(
        "{}:{}",
        message.get("id")?.as_str()?,
        value.get("requestId").and_then(Value::as_str).unwrap_or("")
    );
    let at = value.get("timestamp")?.as_str()?.parse().ok()?;
    Some((
        id,
        Request {
            at,
            model: model.to_owned(),
            tokens: Tokens {
                input: field("input_tokens"),
                output: field("output_tokens"),
                cache_read: field("cache_read_input_tokens"),
                cache_write: field("cache_creation_input_tokens"),
            },
        },
    ))
}
