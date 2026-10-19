//! Open and close projects, and discover or read their numbered entries.

use crate::{
    rail::{self, Change},
    tool::{Answer, Arg, Args, Outcome, Tool, Trouble},
    tools::{PROJECT, entry_of, fields, held, many, root},
};
use artifact::error::{Act, Error};
use artifact::kind::Kind;
use artifact::location::Location;
use artifact::{project::Project as _, reference::Within};
use futures::executor::block_on;
use serde_json::json;

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
        description: "Read a project entry by its numeric reference (#12). An article's part reads that part alone: #12:5-7 its lines, #12#setup the section under a heading. Tables return up to 200 rows with the total count. A card's link (ROAD-12, cydonia://foo#ROAD-12) reads the card, as board_read_card does.",
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
            let mut schema = fields(bound, &[PROJECT, ENTRY, LABELS]);
            many(&mut schema, LABELS);
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

/// A directory on another machine is not made here: the app opens it on its
/// host.
fn open(args: Args<'_>) -> Outcome {
    let location = whole(&args, args.text(PATH)?)?;
    let made = match location.local() {
        Some(path) if !path.is_dir() => {
            // A path that is something other than a directory is a mistake
            // worth reporting rather than one to route around:
            // `create_dir_all` would fail on it anyway, with an error about
            // the parent.
            if path.exists() {
                return Err(Trouble::Refused(format!(
                    "{} is not a directory",
                    path.display()
                )));
            }
            std::fs::create_dir_all(path)
                .map_err(|e| Error::failed(path.display().to_string(), Act::Made, e))?;
            true
        }
        _ => false,
    };
    let location = location.settled();
    // What the app itself does with a project already on the rail — see
    // `Workspace::open_project`. Said plainly, because "opened" would have a
    // model believe it had just made something.
    let did = match (made, rail::is_open(&location)) {
        (true, _) => "made and opened",
        (false, true) => "brought forward",
        (false, false) => "opened",
    };
    rail::ask(Change::Open(location.clone()))?;
    Ok(Answer::said(format!("{did} {location}")).with(json!({
        "path": location,
        "created": made,
    })))
}

/// One project or several. Every path is read and checked open before any of
/// them is closed, so a path that names nothing open refuses the whole call
/// rather than the half that was left.
fn close(args: Args<'_>) -> Outcome {
    let mut shutting: Vec<Location> = Vec::new();
    for named in args.list(PATHS)? {
        let location = whole(&args, named)?.settled();
        if !rail::is_open(&location) {
            return Err(Trouble::Refused(match held() {
                None => format!("{location} is not open, and neither is anything else"),
                Some(open) => format!("{location} is not open — cydonia has {open}"),
            }));
        }
        shutting.push(location);
    }
    let mut shut: Vec<String> = Vec::new();
    for location in &shutting {
        rail::ask(Change::Close(location.clone()))?;
        shut.push(location.to_string());
    }
    Ok(Answer::said(format!("closed {}", shut.join(", "))).with(json!({ "paths": shutting })))
}

// ── the path ─────────────────────────────────────────────────────

/// The directory a call names, as a whole location.
///
/// A relative one is read against the project the caller was opened in, on
/// that project's machine, which is the only thing it could be relative to —
/// the door's own working directory is wherever the app was launched from, and
/// a folder made there would be somewhere nobody asked for. A caller bound to
/// nothing is asked for the whole path instead of guessing.
fn whole(args: &Args<'_>, named: &str) -> Result<Location, Trouble> {
    let location = Location::parse(named);
    if location.root().is_absolute() {
        return Ok(location);
    }
    match args.at() {
        Some(at) => Ok(at.with_root(at.root().join(location.root()))),
        None => Err(Trouble::Refused(format!(
            "{named} is a relative path and this session is not in a project to \
             read it against — give the whole path"
        ))),
    }
}

fn entries(args: Args<'_>) -> Outcome {
    let entries = root(&args)?
        .store
        .entries()
        .map_err(|e| Trouble::Refused(e.to_string()))?;
    let text = if entries.is_empty() {
        "this project has no saved entries".to_owned()
    } else {
        entries
            .iter()
            .map(|entry| {
                format!(
                    "#{} [{}] {}{}{}{}",
                    entry.number,
                    entry.kind,
                    entry.title,
                    match entry.article {
                        Some(article) => format!(" · under #{article}"),
                        None => String::new(),
                    },
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
    // An empty list is an answer here: it takes every label off.
    let labels = artifact::label::normalize_all(args.any_list(LABELS)?);
    let (project, number, within) = entry_of(&args, named)?;
    if within.is_some() {
        return Err(Trouble::Invalid(format!(
            "{named} names part of an entry, and labels go on the whole of one"
        )));
    }
    let entry = super::entry_numbered(&project, number, named)?;
    let store = &project.store;
    let refused = |e: Error| Error::failed(named, Act::Written, e);
    let gone = || Error::Gone {
        named: named.to_owned(),
    };
    match entry.kind {
        Kind::Article => {
            let mut properties = store.properties(&entry.id);
            properties.labels = labels.clone();
            block_on(store.save_properties(&entry.id, &properties)).map_err(refused)?;
        }
        Kind::Board => {
            let mut board = store.board(&entry.id).ok_or_else(gone)?;
            board.labels = labels.clone();
            block_on(store.save_board(&mut board)).map_err(refused)?;
        }
        // An open project's sessions are the app's to write: a file written
        // under it would be overwritten by the app's next save.
        Kind::Session if project.is_open() => rail::ask(Change::Label {
            session: entry.id.clone(),
            labels: labels.clone(),
        })?,
        Kind::Session => {
            let mut record = store.session(&entry.id).ok_or_else(gone)?;
            record.labels = labels.clone();
            block_on(store.save_session(&record)).map_err(refused)?;
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
    if let Some(reference) = artifact::reference::parse(named)
        && let artifact::reference::Target::Card { .. } = reference.target
    {
        let project = super::project_of(&args, &reference)?;
        return super::board::cards(&project, &[named]);
    }
    let (project, number, within) = entry_of(&args, named)?;
    let entry = super::entry_numbered(&project, number, named)?;
    artifact::reference::fits(named, entry.kind, within)?;
    if entry.kind == Kind::Board {
        return super::board::listed(&project, &entry.id);
    }
    if let (Kind::Session, Some(Within::Span(_))) = (entry.kind, within) {
        return Err(Trouble::Invalid(format!(
            "{named} names turns — read them with session_read"
        )));
    }
    let content = project
        .store
        .read(&entry)
        .map_err(|e| Trouble::Refused(e.to_string()))?;

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
