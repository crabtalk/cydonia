//! Open and close projects, and discover or read their numbered entries.

use crate::{
    rail::{self, Change},
    tool::{Answer, Arg, Args, Outcome, Tool, Trouble},
    tools::{PROJECT, entry_of, fields, held, many, root},
};
use artifact::{
    project::{Project as _, fs},
    reference::Within,
};
use serde_json::json;
use std::path::{Path, PathBuf};

const PATH: Arg = Arg {
    name: "path",
    about: "The project's directory, as a whole path — or one relative \
to the project this session is already in.",
};
/// The same argument where several are taken at once. A second const rather
/// than a flag on [`PATH`], the way `board::CARDS` stands beside `board::CARD`.
const PATHS: Arg = Arg {
    name: "path",
    about: "The project's directory, or several: each a whole path, or one \
relative to the project this session is already in.",
};

const ENTRY: Arg = Arg {
    name: "entry",
    about: "The project entry reference: #12, foo#12 for another open project, or the link cydonia://foo#12.",
};

const LABELS: Arg = Arg {
    name: "labels",
    about: "Every label the entry should carry, replacing what it had: a name or a list of names, empty to take them all off.",
};

pub static TOOLS: [Tool; 5] = [
    Tool {
        name: "project_entries",
        description: "List articles, boards, tables, and saved chats with stable project-wide numeric references and their labels, including archived entries.",
        schema: |bound| fields(bound, &[PROJECT]),
        writes: false,
        deletes: false,
        call: entries,
    },
    Tool {
        name: "project_read_entry",
        description: "Read a project entry by its numeric reference (#12). An article's part reads that part alone: #12:5-7 its lines, #12#setup the section under a heading. Tables return up to 200 rows with the total count.",
        schema: |bound| fields(bound, &[PROJECT, ENTRY]),
        writes: false,
        deletes: false,
        call: read_entry,
    },
    Tool {
        name: "project_label_entry",
        description: "Set the labels an article, board or saved chat carries. Names are lowercased, \
            with spaces as -, and shared across projects. Tables carry no labels.",
        schema: |bound| {
            let mut schema = fields(bound, &[PROJECT, ENTRY]);
            schema["properties"][LABELS.name] = json!({
                "anyOf": [
                    { "type": "string" },
                    { "type": "array", "items": { "type": "string" } }
                ],
                "description": LABELS.about,
            });
            schema["required"] = json!([ENTRY.name, LABELS.name]);
            schema
        },
        writes: true,
        deletes: false,
        call: label_entry,
    },
    Tool {
        name: "project_open",
        description: "Open a directory as a project in cydonia, making the \
            directory first if it is not there yet.",
        schema: |bound| fields(bound, &[PATH]),
        writes: true,
        deletes: false,
        call: open,
    },
    Tool {
        name: "project_close",
        description: "Close a project in cydonia, or several in one call: each leaves the sidebar and its \
            agents stop. Nothing on disk is touched.",
        schema: |bound| {
            let mut schema = fields(bound, &[PATHS]);
            many(&mut schema, PATHS);
            schema
        },
        writes: true,
        deletes: false,
        call: close,
    },
];

// ── the tools ────────────────────────────────────────────────────

fn open(args: Args<'_>) -> Outcome {
    let path = whole(&args, args.text(PATH)?)?;
    let made = !path.is_dir();
    if made {
        // A path that is something other than a directory is a mistake worth
        // reporting rather than one to route around: `create_dir_all` would
        // fail on it anyway, with an error about the parent.
        if path.exists() {
            return Err(Trouble::Refused(format!(
                "{} is not a directory",
                path.display()
            )));
        }
        std::fs::create_dir_all(&path)
            .map_err(|e| Trouble::Refused(format!("{} cannot be made — {e}", path.display())))?;
    }
    let path = settled(path);
    // What the app itself does with a project already on the rail — see
    // `Workspace::open_project`. Said plainly, because "opened" would have a
    // model believe it had just made something.
    let did = match (made, rail::is_open(&path)) {
        (true, _) => "made and opened",
        (false, true) => "brought forward",
        (false, false) => "opened",
    };
    rail::ask(Change::Open(path.clone()))?;
    Ok(
        Answer::said(format!("{did} {}", path.display())).with(json!({
            "path": path,
            "created": made,
        })),
    )
}

/// One project or several. Every path is read and checked open before any of
/// them is closed, so a path that names nothing open refuses the whole call
/// rather than the half that was left.
fn close(args: Args<'_>) -> Outcome {
    let mut shutting: Vec<PathBuf> = Vec::new();
    for named in args.list(PATHS)? {
        let path = settled(whole(&args, named)?);
        if !rail::is_open(&path) {
            return Err(Trouble::Refused(match held() {
                None => format!(
                    "{} is not open, and neither is anything else",
                    path.display()
                ),
                Some(open) => format!("{} is not open — cydonia has {open}", path.display()),
            }));
        }
        shutting.push(path);
    }
    let mut shut: Vec<String> = Vec::new();
    for path in &shutting {
        rail::ask(Change::Close(path.clone()))?;
        shut.push(path.display().to_string());
    }
    Ok(Answer::said(format!("closed {}", shut.join(", "))).with(json!({ "paths": shutting })))
}

// ── the path ─────────────────────────────────────────────────────

/// The directory a call names, as a whole path.
///
/// A relative one is read against the project the caller was opened in, which
/// is the only thing it could be relative to — the door's own working directory
/// is wherever the app was launched from, and a folder made there would be
/// somewhere nobody asked for. A caller bound to nothing is asked for the whole
/// path instead of guessing.
fn whole(args: &Args<'_>, named: &str) -> Result<PathBuf, Trouble> {
    let path = Path::new(named);
    if path.is_absolute() {
        return Ok(path.to_path_buf());
    }
    match args.at() {
        Some(at) => Ok(at.join(path)),
        None => Err(Trouble::Refused(format!(
            "{named} is a relative path and this session is not in a project to \
             read it against — give the whole path"
        ))),
    }
}

/// The path with `..`, `.` and any symlink on the way resolved, where the
/// directory is there to resolve it against.
///
/// Two spellings of one directory are two projects as far as a list of paths
/// can tell, and the rail is a list of paths. Left alone where it cannot be
/// resolved, which is the path that was asked about — the honest answer to
/// `project_close` on a directory that has since been deleted.
fn settled(path: PathBuf) -> PathBuf {
    path.canonicalize().unwrap_or(path)
}

fn entries(args: Args<'_>) -> Outcome {
    let entries =
        artifact::entry::list(root(&args)?).map_err(|e| Trouble::Refused(e.to_string()))?;
    let text = if entries.is_empty() {
        "this project has no saved entries".to_owned()
    } else {
        entries
            .iter()
            .map(|entry| {
                format!(
                    "#{} [{}] {}{}{}",
                    entry.number,
                    entry.kind,
                    entry.title,
                    match entry.labels.is_empty() {
                        true => String::new(),
                        false => format!(" · labels: {}", entry.labels.join(", ")),
                    },
                    if entry.archived { " — archived" } else { "" }
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    };
    Ok(Answer::said(text).with(json!({"entries": entries})))
}

fn label_entry(args: Args<'_>) -> Outcome {
    let named = args.text(ENTRY)?;
    let wrong = || {
        Trouble::Invalid(format!(
            "{} is required, as a string or a list of strings",
            LABELS.name
        ))
    };
    let labels = match args.value(LABELS) {
        Some(serde_json::Value::String(one)) => vec![one.as_str()],
        Some(serde_json::Value::Array(many)) => many
            .iter()
            .map(|value| value.as_str().ok_or_else(wrong))
            .collect::<Result<_, _>>()?,
        _ => return Err(wrong()),
    };
    let labels = artifact::label::normalize_all(labels);
    let (project, number, within) = entry_of(&args, named)?;
    if within.is_some() {
        return Err(Trouble::Invalid(format!(
            "{named} names part of an entry, and labels go on the whole of one"
        )));
    }
    let entries = artifact::entry::list(&project).map_err(|e| Trouble::Refused(e.to_string()))?;
    let entry = entries
        .into_iter()
        .find(|entry| entry.number == number)
        .ok_or_else(|| Trouble::Refused(format!("no entry {named} in this project")))?;
    let store = fs::Project::new(&project);
    let refused =
        |e: &dyn std::fmt::Display| Trouble::Refused(format!("{named} cannot be written — {e}"));
    match entry.kind {
        "article" => {
            let mut properties = store.properties(&entry.id);
            properties.labels = labels.clone();
            store
                .save_properties(&entry.id, &properties)
                .map_err(|e| refused(&e))?;
        }
        "board" => {
            let mut board = store
                .board(&entry.id)
                .ok_or_else(|| Trouble::Refused(format!("{named} no longer exists")))?;
            board.labels = labels.clone();
            store.save_board(&mut board).map_err(|e| refused(&e))?;
        }
        // An open project's sessions are the app's to write: a file written
        // under it would be overwritten by the app's next save.
        "session" if rail::is_open(&project) => rail::ask(Change::Label {
            session: entry.id.clone(),
            labels: labels.clone(),
        })?,
        "session" => {
            let mut record = store
                .session(&entry.id)
                .ok_or_else(|| Trouble::Refused(format!("{named} no longer exists")))?;
            record.labels = labels.clone();
            store.save_session(&record).map_err(|e| refused(&e))?;
        }
        kind => {
            return Err(Trouble::Refused(format!(
                "{named} is a {kind}, and only articles, boards and saved chats carry labels"
            )));
        }
    }
    let said = match labels.is_empty() {
        true => format!("#{number} {} has no labels", entry.title),
        false => format!("#{number} {} labelled {}", entry.title, labels.join(", ")),
    };
    Ok(Answer::said(said).with(json!({"number": number, "labels": labels})))
}

fn read_entry(args: Args<'_>) -> Outcome {
    let named = args.text(ENTRY)?;
    let (project, number, within) = entry_of(&args, named)?;
    let project = project.as_path();
    let entries = artifact::entry::list(project).map_err(|e| Trouble::Refused(e.to_string()))?;
    let entry = entries
        .into_iter()
        .find(|entry| entry.number == number)
        .ok_or_else(|| Trouble::Refused(format!("no entry {named} in this project")))?;
    if within.is_some() && entry.kind != "article" {
        return Err(match (entry.kind, within) {
            ("session", Some(Within::Span(_))) => {
                Trouble::Invalid(format!("{named} names turns — read them with session_read"))
            }
            (kind, _) => Trouble::Invalid(format!(
                "{named} names part of a {kind}, and only an article's lines or a session's turns can be named"
            )),
        });
    }
    let content =
        artifact::entry::read(project, &entry).map_err(|e| Trouble::Refused(e.to_string()))?;
    let markdown = content.get("markdown").and_then(serde_json::Value::as_str);
    if let (Some(within), Some(markdown)) = (within, markdown) {
        let (range, lines) = super::article::excerpt(markdown, within, named)?;
        return Ok(Answer::said(&markdown[range]).with(json!({
            "entry": entry,
            "lines": { "from": lines.from, "to": lines.to },
        })));
    }
    let text = markdown
        .map(str::to_owned)
        .unwrap_or_else(|| serde_json::to_string_pretty(&content).unwrap_or_default());
    Ok(Answer::said(text).with(json!({"entry": entry, "content": content})))
}
