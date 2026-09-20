//! Open and close projects, and discover or read their numbered entries.

use crate::{
    rail::{self, Change},
    tool::{Answer, Arg, Args, Outcome, Tool, Trouble},
    tools::{PROJECT, fields, held, many, root},
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
    about: "The project entry reference, such as #12.",
};

pub static TOOLS: [Tool; 4] = [
    Tool {
        name: "project_entries",
        description: "List articles, boards, tables, and saved chats with stable project-wide numeric references, including archived entries.",
        schema: |bound| fields(bound, &[PROJECT]),
        writes: false,
        call: entries,
    },
    Tool {
        name: "project_read_entry",
        description: "Read a project entry by its numeric reference (#12). Tables return up to 200 rows with the total count.",
        schema: |bound| fields(bound, &[PROJECT, ENTRY]),
        writes: false,
        call: read_entry,
    },
    Tool {
        name: "project_open",
        description: "Open a directory as a project in cydonia, making the \
            directory first if it is not there yet.",
        schema: |bound| fields(bound, &[PATH]),
        writes: true,
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
                    "#{} [{}] {}{}",
                    entry.number,
                    entry.kind,
                    entry.title,
                    if entry.archived { " — archived" } else { "" }
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    };
    Ok(Answer::said(text).with(json!({"entries": entries})))
}

fn read_entry(args: Args<'_>) -> Outcome {
    let project = root(&args)?;
    let named = args.text(ENTRY)?;
    let number = artifact::entry::reference(named)
        .ok_or_else(|| Trouble::Invalid("entry must be a reference such as #12".to_owned()))?;
    let entries = artifact::entry::list(project).map_err(|e| Trouble::Refused(e.to_string()))?;
    let entry = entries
        .into_iter()
        .find(|entry| entry.number == number)
        .ok_or_else(|| Trouble::Refused(format!("no entry {named} in this project")))?;
    let content =
        artifact::entry::read(project, &entry).map_err(|e| Trouble::Refused(e.to_string()))?;
    let text = content
        .get("markdown")
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned)
        .unwrap_or_else(|| serde_json::to_string_pretty(&content).unwrap_or_default());
    Ok(Answer::said(text).with(json!({"entry": entry, "content": content})))
}
