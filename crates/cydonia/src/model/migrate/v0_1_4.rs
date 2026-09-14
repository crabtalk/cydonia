//! 0.1.4 — two carry-overs, both onto `settings.toml`.
//!
//! **The presentation preferences, out of `state.toml`.** Up to 0.1.3 the
//! theme, the caret's blink, the type size, the greys' tint and the page width
//! were written into `state.toml` beside the list of open projects.
//! `state.toml` is machine-written bookkeeping full of absolute paths: not
//! worth hand-editing, and not portable to another machine. Those preferences
//! are both. They live in `settings.toml` under `[appearance]` now — see
//! [`crate::model::settings::Appearance`].
//!
//! **The agents nobody installed, out of `settings.toml`.** Up to 0.1.3 a
//! fresh install wrote an `npx` line for claude and one for codex, and opening
//! a project started a session on the first of them. Neither was ever on the
//! machine: the line alone was enough to offer the agent in the picker and to
//! fetch and run its package on npm. Agents arrive through Settings › Agents
//! now and nowhere else, so the two lines come back out.
//!
//! Retire this once nobody upgrading can still be on 0.1.3 or earlier. See
//! [`super`].

use crate::model::{settings, state};
use toml_edit::DocumentMut;

/// The keys that moved, in the order they are written back out. Named once, so
/// the carry and its test cannot drift apart.
const MOVED: [&str; 8] = [
    "appearance",
    "opaque",
    "cursor_blink",
    "text_size",
    "hue",
    "chroma",
    "wide_pages",
    "wrap_code",
];

/// What the key was called in `state.toml`, where the two disagree.
///
/// `appearance` was a bare key naming light or dark. In `settings.toml` that
/// is the table's own name, so the value inside it is `mode`.
fn renamed(key: &str) -> &str {
    match key {
        "appearance" => "mode",
        key => key,
    }
}

/// Perform both carry-overs. Best effort — see [`super::run`].
///
/// `settings.toml` is the one file both halves write, so both are decided
/// before it is written once. A `state.toml` that is missing or unreadable is
/// not a reason to skip the agents: a person who never ran 0.1.3 long enough
/// to have one can still have the two lines.
pub fn run() {
    let Ok(settings_path) = settings::path() else {
        return;
    };
    let settings_body = std::fs::read_to_string(&settings_path).unwrap_or_default();
    let Ok(mut settings) = settings_body.parse::<DocumentMut>() else {
        return;
    };
    let state_path = state::path();
    let mut state = state_path
        .as_ref()
        .and_then(|path| std::fs::read_to_string(path).ok())
        .and_then(|body| body.parse::<DocumentMut>().ok());
    let carried = state
        .as_mut()
        .is_some_and(|state| carry_appearance(state, &mut settings));
    let dropped = drop_default_agents(&mut settings);
    if !carried && !dropped {
        return;
    }
    // Settings first. Written the other way round, a crash between the two
    // would take the preferences with it; this way the worst case is a
    // `state.toml` still holding keys nothing reads, which the next launch
    // clears.
    if let Some(dir) = settings_path.parent()
        && std::fs::create_dir_all(dir).is_ok()
        && std::fs::write(&settings_path, settings.to_string()).is_ok()
        && let (true, Some(path), Some(state)) = (carried, state_path, state)
    {
        let _ = std::fs::write(path, state.to_string());
    }
}

/// The move itself, over the two parsed documents. Answers whether anything
/// changed.
///
/// Pure over the documents so it can be tested without a config directory —
/// the file handling above is the part that cannot be.
///
/// A key already in `[appearance]` wins: the settings file is the one a person
/// edits, and a stale copy left in `state.toml` must never overwrite what they
/// wrote. That is also what makes this safe to run on every launch.
pub fn carry_appearance(state: &mut DocumentMut, settings: &mut DocumentMut) -> bool {
    let mut moved = false;
    for key in MOVED {
        let Some(value) = state.get(key).and_then(|held| held.as_value()).cloned() else {
            continue;
        };
        // Made only once something is actually there to put in it, so a
        // `state.toml` with none of these leaves no empty table behind.
        let table = settings["appearance"].or_insert(toml_edit::table());
        let Some(table) = table.as_table_mut() else {
            return false;
        };
        table.set_implicit(false);
        if !table.contains_key(renamed(key)) {
            table[renamed(key)] = toml_edit::value(value);
        }
        state.remove(key);
        moved = true;
    }
    moved
}

/// The two entries a fresh install used to write, as the name and the npm
/// package that were paired in it. The version moved from release to release —
/// the pair did not, which is why the version is not part of the match.
const DEFAULTED: [(&str, &str); 2] = [
    ("claude", "@agentclientprotocol/claude-agent-acp"),
    ("codex", "@agentclientprotocol/codex-acp"),
];

/// Take the agents nobody installed back out of `settings.toml`. Answers
/// whether anything went.
///
/// Pure over the document for the same reason [`carry_appearance`] is.
///
/// Matched on the whole shape of the table rather than on the package alone,
/// because an `npx` line does work — it fetches the package and runs it — and
/// one somebody wrote on purpose is theirs to keep. What is dropped is
/// byte-for-byte what cydonia wrote: the shipped name beside its own package,
/// no `env`, and no `id`. That last is the one that carries the argument. An
/// install through Settings › Agents *replaces* the line for its package and
/// writes the registry id onto it — see [`crate::model::settings::put_agent`]
/// — so an entry still without one is an entry no install ever stood behind.
pub fn drop_default_agents(settings: &mut DocumentMut) -> bool {
    let Some(agents) = settings
        .get_mut("agents")
        .and_then(|item| item.as_array_of_tables_mut())
    else {
        return false;
    };
    // The file's header hangs off whichever table comes first. If that is one
    // of the two, the header has to move down onto its successor or it leaves
    // with it — the same care [`crate::model::settings::remove_agent`] takes.
    //
    // Only where it says something: blank lines are the spacing around the
    // entry being dropped, and carrying those down puts a gap where the entry
    // was rather than closing it.
    let prefix = agents
        .get(0)
        .filter(|table| defaulted(table))
        .map(|table| table.decor().prefix())
        .filter(|prefix| {
            prefix.is_some_and(|held| held.as_str().is_some_and(|text| !text.trim().is_empty()))
        })
        .and_then(|prefix| prefix.cloned());
    let before = agents.len();
    agents.retain(|table| !defaulted(table));
    if agents.len() == before {
        return false;
    }
    if let Some(prefix) = prefix {
        match agents.get_mut(0) {
            Some(first) => first.decor_mut().set_prefix(prefix),
            None => settings.as_table_mut().decor_mut().set_prefix(prefix),
        }
    }
    true
}

/// Whether the table is one of the two, untouched since cydonia wrote it.
fn defaulted(table: &toml_edit::Table) -> bool {
    if table.contains_key("id") || table.contains_key("env") {
        return false;
    }
    let field = |key| table.get(key).and_then(|value| value.as_str());
    if field("command") != Some("npx") {
        return false;
    }
    let Some(args) = table.get("args").and_then(|args| args.as_array()) else {
        return false;
    };
    let args: Vec<&str> = args.iter().filter_map(|arg| arg.as_str()).collect();
    let [flag, spec] = args[..] else {
        return false;
    };
    let package = cacp_agents::package_name(spec);
    flag == "-y" && DEFAULTED.contains(&(field("name").unwrap_or_default(), package))
}
