//! Talk to the agent sessions a project holds.

use crate::{
    rail::{self, Change},
    tool::{Answer, Arg, Args, Outcome, Tool, Trouble},
    tools::{PROJECT, fields, project_of, root},
};
use artifact::{
    project::fs,
    reference::{self, Reference, Target, Turns},
    session::{
        chat::{self, ChatItem, ToolStatus},
        record::Record,
    },
};
use serde_json::json;
use std::path::Path;

const SESSION: Arg = Arg {
    name: "session",
    about: "The session's project entry reference, such as #43. Leave it out \
to start a new session on `agent` instead.",
};

const AGENT: Arg = Arg {
    name: "agent",
    about: "The agent to start a new session on, by its configured name or id. \
Used only when `session` is left out.",
};

const MESSAGE: Arg = Arg {
    name: "message",
    about: "What to send, as the session's next prompt.",
};

/// The session to read or search, as a reference.
const READ: Arg = Arg {
    name: "session",
    about: "The session: its reference, such as #43, optionally with turns \
(#43:5 or #43:5-7), and optionally in another open project (foo#43:5-7).",
};

const SEARCHED: Arg = Arg {
    name: "session",
    about: "The session to search, such as #43 or foo#43. Leave it out to \
search every session in the project.",
};

const TURNS: Arg = Arg {
    name: "turns",
    about: "The turns to read, counted from 1: 5, or 5-7 for a run. Overrides \
turns written on the session reference. Without either, the last 3 turns.",
};

const QUERY: Arg = Arg {
    name: "query",
    about: "The text to find, matched case-insensitively.",
};

const FULL: Arg = Arg {
    name: "full",
    about: "Include thinking, tool output and agent process output. Off by \
default: each tool call is one line.",
};

/// Turns read when none are asked for.
const LATEST: u64 = 3;

/// Hits a search answers with, at most.
const HITS: usize = 20;

/// Characters of a hit's line shown, at most.
const SNIPPET: usize = 160;

pub static TOOLS: [Tool; 3] = [
    Tool {
        name: "session_send",
        description: "Send a message to another agent session in the project, named by its entry \
        reference (#43), or start a new session on a named agent with the message as its first \
        prompt. Fire and forget: nothing is waited for or answered back.",
        schema: |bound| {
            let mut schema = fields(bound, &[PROJECT, SESSION, AGENT, MESSAGE]);
            let agents = rail::agents();
            if !agents.is_empty() {
                schema["properties"][AGENT.name] = json!({
                    "type": "string",
                    "enum": agents.iter().map(rail::Agent::key).collect::<Vec<_>>(),
                    "description": format!("{} Configured: {}.", AGENT.about, listed(&agents)),
                });
            }
            schema["required"] = json!(match bound {
                true => vec![MESSAGE.name],
                false => vec![PROJECT.name, MESSAGE.name],
            });
            schema
        },
        writes: true,
        deletes: false,
        call: send,
    },
    Tool {
        name: "session_read",
        description: "Read turns of a session by reference: #43:5-7, or the session and a \
            `turns` range. A turn is one message sent to the agent and everything it did in \
            answer. Reads archived sessions too. Without turns, the last 3.",
        schema: |bound| {
            let mut schema = fields(bound, &[PROJECT, READ]);
            schema["properties"][TURNS.name] =
                json!({ "type": "string", "description": TURNS.about });
            schema["properties"][FULL.name] =
                json!({ "type": "boolean", "description": FULL.about });
            schema
        },
        writes: false,
        deletes: false,
        call: read,
    },
    Tool {
        name: "session_search",
        description: "Find text in one session, or in every session of the project, archived \
            ones included. Answers each matching turn as a reference such as #43:5 with the \
            line it matched, ready for session_read.",
        schema: |bound| {
            let mut schema = fields(bound, &[PROJECT, QUERY]);
            schema["properties"][SEARCHED.name] =
                json!({ "type": "string", "description": SEARCHED.about });
            schema
        },
        writes: false,
        deletes: false,
        call: search,
    },
];

fn send(args: Args<'_>) -> Outcome {
    let project = root(&args)?;
    let message = args.text(MESSAGE)?;
    if message.trim().is_empty() {
        return Err(Trouble::Refused("message is empty".to_owned()));
    }
    // Sessions run in the app, and only for projects on the rail.
    if !rail::is_open(project) {
        return Err(Trouble::Refused(format!(
            "cydonia does not have {} open, so no session there can be sent to",
            project.display()
        )));
    }
    let signed = match sender(&args, project) {
        Some(from) => format!("from {from}\n\n{message}"),
        None => message.to_owned(),
    };
    let message = signed.as_str();
    let Some(named) = args.maybe(SESSION) else {
        return start(project, args.maybe(AGENT), message);
    };
    let number = artifact::entry::reference(named)
        .ok_or_else(|| Trouble::Invalid("session must be a reference such as #43".to_owned()))?;
    let entry = artifact::entry::list(project)
        .map_err(|e| Trouble::Refused(e.to_string()))?
        .into_iter()
        .find(|entry| entry.number == number)
        .ok_or_else(|| Trouble::Refused(format!("no entry {named} in this project")))?;
    if entry.kind != "session" {
        return Err(Trouble::Refused(format!(
            "{named} is a {}, not a session",
            entry.kind
        )));
    }
    rail::ask(Change::Send {
        session: entry.id.clone(),
        message: message.to_owned(),
    })?;
    Ok(Answer::said(format!("sent to #{number} {}", entry.title)))
}

/// The calling session as a reference to the turn it is on — `#42:7`, or
/// `foo#42:7` when the message leaves its project. Nothing for a caller that is
/// not a session, or one with no turn on disk yet.
fn sender(args: &Args<'_>, to: &Path) -> Option<String> {
    let (at, record) = (args.at()?, args.session()?);
    let turn = chat::turns(&fs::Project::new(at).session(record)?.items).len();
    let number = artifact::entry::number(at, "session", record).ok()?;
    let project = match same_dir(at, to) {
        true => String::new(),
        false => at.file_name()?.to_string_lossy().into_owned(),
    };
    (turn > 0).then(|| format!("{project}#{number}:{turn}"))
}

fn same_dir(a: &Path, b: &Path) -> bool {
    let settled = |path: &Path| path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    settled(a) == settled(b)
}

/// A new session on `agent`, seeded with `message`.
fn start(project: &Path, agent: Option<&str>, message: &str) -> Outcome {
    let named = agent
        .ok_or_else(|| Trouble::Invalid("session or agent is required, as a string".to_owned()))?;
    let agents = rail::agents();
    let agent = agents
        .iter()
        .find(|agent| agent.id.as_deref() == Some(named))
        .or_else(|| {
            agents
                .iter()
                .find(|agent| agent.name.eq_ignore_ascii_case(named))
        })
        .ok_or_else(|| {
            Trouble::Refused(match agents.is_empty() {
                true => format!("no agent named {named}, and cydonia has none configured"),
                false => format!("no agent named {named} — cydonia has {}", listed(&agents)),
            })
        })?;
    let project = project
        .canonicalize()
        .unwrap_or_else(|_| project.to_path_buf());
    rail::ask(Change::Start {
        project,
        agent: agent.key().to_owned(),
        message: message.to_owned(),
    })?;
    Ok(Answer::said(format!(
        "sent to a new session on {}",
        agent.name
    )))
}

/// `Claude Agent (claude-acp), Codex (codex-acp)`.
fn listed(agents: &[rail::Agent]) -> String {
    agents
        .iter()
        .map(|agent| match &agent.id {
            Some(id) => format!("{} ({id})", agent.name),
            None => agent.name.clone(),
        })
        .collect::<Vec<_>>()
        .join(", ")
}

// ── reading ──────────────────────────────────────────────────────

/// A session a reference names, read off disk.
struct Found {
    number: u64,
    title: String,
    record: Record,
    turns: Option<Turns>,
}

/// The session `named` refers to, in whichever project it names.
fn found(args: &Args<'_>, named: &str) -> Result<Found, Trouble> {
    let reference = reference::parse(named).ok_or_else(|| {
        Trouble::Invalid(format!(
            "{named} is not a session reference — write #43, #43:5-7 or foo#43"
        ))
    })?;
    let Reference {
        target: Target::Entry { number, turns },
        ..
    } = reference
    else {
        return Err(Trouble::Refused(format!(
            "{named} is a card, not a session"
        )));
    };
    let project = project_of(args, &reference)?;
    // Resolved through the registry alone: listing the project's entries would
    // parse every session file to find one.
    let id = artifact::entry::Registry::open(&project)
        .and_then(|registry| registry.resolve("session", number))
        .map_err(|e| Trouble::Refused(e.to_string()))?
        .ok_or_else(|| Trouble::Refused(format!("no session #{number} in this project")))?;
    let record = fs::Project::new(&project)
        .session(&id)
        .ok_or_else(|| Trouble::Refused(format!("session #{number} cannot be read")))?;
    Ok(Found {
        number,
        title: record.name.clone().unwrap_or_else(|| record.title.clone()),
        record,
        turns,
    })
}

fn read(args: Args<'_>) -> Outcome {
    let named = args.text(READ)?;
    let full = args.boolean(FULL, false)?;
    let found = found(&args, named)?;
    let asked = match args.maybe(TURNS) {
        Some(turns) => Some(range(turns)?),
        None => found.turns,
    };
    let turns = chat::turns(&found.record.items);
    let count = turns.len() as u64;
    if count == 0 {
        return Ok(Answer::said(format!(
            "#{} {} has no turns yet",
            found.number, found.title
        )));
    }
    let run = asked.unwrap_or(Turns {
        from: count.saturating_sub(LATEST - 1).max(1),
        to: count,
    });
    if run.from > count {
        return Err(Trouble::Refused(format!(
            "#{} has {count} turns, so there is no turn {}",
            found.number, run.from
        )));
    }
    let run = Turns {
        from: run.from,
        to: run.to.min(count),
    };
    let number = found.number;
    let mut text = format!("#{number} {} — turns {run} of {count}", found.title);
    for turn in run.from..=run.to {
        let items = &found.record.items[turns[turn as usize - 1].clone()];
        text.push_str(&format!("\n\n## #{number}:{turn}\n"));
        for item in items {
            if let Some(line) = rendered(item, full) {
                text.push('\n');
                text.push_str(&line);
            }
        }
    }
    Ok(Answer::said(text).with(json!({
        "session": number,
        "title": found.title,
        "turns": { "from": run.from, "to": run.to },
        "count": count,
    })))
}

/// One item as the model reads it, or nothing for what is left out unless
/// `full` is asked for.
fn rendered(item: &ChatItem, full: bool) -> Option<String> {
    Some(match item {
        ChatItem::User(text) => format!("user: {text}"),
        ChatItem::Agent(text) => format!("agent: {text}"),
        ChatItem::Thinking { text, .. } => {
            if !full {
                return None;
            }
            format!("thinking: {text}")
        }
        ChatItem::Tool {
            label,
            status,
            output,
            ..
        } => {
            let status = match status {
                ToolStatus::Running => "running",
                ToolStatus::Success => "ok",
                ToolStatus::Failure => "failed",
            };
            match full && !output.is_empty() {
                true => format!("tool: {label} — {status}\n{output}"),
                false => format!("tool: {label} — {status}"),
            }
        }
        ChatItem::Notice { text, .. } => format!("notice: {text}"),
        ChatItem::Process { command, output } => {
            if !full {
                return None;
            }
            format!("process: {command}\n{output}")
        }
    })
}

/// `5` or `5-7`, read the way a reference writes its turns.
fn range(text: &str) -> Result<Turns, Trouble> {
    match reference::parse(&format!("#1:{text}")) {
        Some(Reference {
            target: Target::Entry {
                turns: Some(turns), ..
            },
            ..
        }) => Ok(turns),
        _ => Err(Trouble::Invalid(format!(
            "turns must be a turn such as 5, or a run such as 5-7 — not {text}"
        ))),
    }
}

// ── searching ────────────────────────────────────────────────────

fn search(args: Args<'_>) -> Outcome {
    let query = args.text(QUERY)?.trim();
    if query.is_empty() {
        return Err(Trouble::Refused("query is empty".to_owned()));
    }
    let pattern = literal(query);
    let searched: Box<dyn Iterator<Item = (u64, Record)>> = match args.maybe(SEARCHED) {
        Some(named) => {
            let found = found(&args, named)?;
            Box::new(std::iter::once((found.number, found.record)))
        }
        None => Box::new(grepped(root(&args)?, &in_json(query))),
    };
    let mut hits = Vec::new();
    let mut more = false;
    'sessions: for (number, record) in searched {
        let title = record.name.as_deref().unwrap_or(&record.title);
        for (at, turn) in chat::turns(&record.items).into_iter().enumerate() {
            let Some(line) = record.items[turn]
                .iter()
                .filter_map(|item| searchable(item))
                .flat_map(str::lines)
                .find(|line| pattern.is_match(line.as_bytes()))
            else {
                continue;
            };
            if hits.len() == HITS {
                more = true;
                break 'sessions;
            }
            hits.push(json!({
                "reference": format!("#{number}:{}", at + 1),
                "title": title,
                "line": snippet(line),
            }));
        }
    }
    if hits.is_empty() {
        return Ok(Answer::said(format!("nothing matches {query}")).with(json!({ "hits": [] })));
    }
    let mut text = hits
        .iter()
        .map(|hit| {
            format!(
                "{} {} — {}",
                hit["reference"].as_str().unwrap_or_default(),
                hit["title"].as_str().unwrap_or_default(),
                hit["line"].as_str().unwrap_or_default()
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    if more {
        text.push_str(&format!(
            "\n… more than {HITS} turns match; narrow the query or name a session"
        ));
    }
    Ok(Answer::said(text).with(json!({ "hits": hits, "more": more })))
}

/// `text` as a case-insensitive literal.
fn literal(text: &str) -> regex::bytes::Regex {
    regex::bytes::RegexBuilder::new(&regex::escape(text))
        .case_insensitive(true)
        .build()
        .expect("an escaped literal is a valid pattern")
}

/// `query` as it appears inside a session file: written the way serde_json
/// writes a string, with quotes, backslashes and control characters escaped
/// and everything else as is.
fn in_json(query: &str) -> regex::bytes::Regex {
    let escaped = serde_json::to_string(query).unwrap_or_default();
    literal(&escaped[1..escaped.len() - 1])
}

/// Every session in the project whose file holds `pattern` anywhere, archived
/// ones included, in entry order. A file is parsed only when its raw bytes
/// match, and only as the iterator reaches it.
fn grepped(
    project: &Path,
    pattern: &regex::bytes::Regex,
) -> impl Iterator<Item = (u64, Record)> + use<> {
    let mut matched: Vec<(u64, String, Vec<u8>)> = fs::Project::new(project)
        .session_files()
        .into_iter()
        .filter_map(|(id, path)| {
            let bytes = std::fs::read(&path).ok()?;
            if !pattern.is_match(&bytes) {
                return None;
            }
            let number = artifact::entry::number(project, "session", &id).ok()?;
            Some((number, id, bytes))
        })
        .collect();
    matched.sort_by_key(|(number, ..)| *number);
    matched.into_iter().filter_map(|(number, id, bytes)| {
        let mut record: Record = serde_json::from_slice(&bytes).ok()?;
        if record.id.is_empty() {
            record.id = id;
        }
        Some((number, record))
    })
}

/// The text of an item a search looks through: what was said, and what each
/// tool call was.
fn searchable(item: &ChatItem) -> Option<&str> {
    match item {
        ChatItem::User(text) | ChatItem::Agent(text) => Some(text),
        ChatItem::Tool { label, .. } => Some(label),
        ChatItem::Notice { text, .. } => Some(text),
        ChatItem::Thinking { .. } | ChatItem::Process { .. } => None,
    }
}

fn snippet(line: &str) -> String {
    let line = line.trim();
    match line.char_indices().nth(SNIPPET) {
        Some((cut, _)) => format!("{}…", &line[..cut]),
        None => line.to_owned(),
    }
}
