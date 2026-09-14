//! The tools an agent works the app's rail of projects through.
//!
//! Every other tool set here is about what a directory holds and reaches one
//! whether the app has it open or not. These two are the ones that decide that:
//! a project is a directory the app is showing, with a place in the sidebar and
//! somewhere to run agents.
//!
//! Named for what the app calls the same two acts — its File menu opens and
//! closes a project, and a person reading the transcript should recognise what
//! the agent just did to their window.
//!
//! Opening makes the directory if it is not there yet, which is the one thing
//! these do to a disk. Closing does nothing to one: the project comes off the
//! rail, its agents stop, and the files and everything cydonia kept beside them
//! stay exactly where they are — opening it again brings all of it back. There
//! is no tool here that deletes anything, and that is on purpose.

use crate::{
    rail::{self, Change},
    tool::{Answer, Args, Outcome, Tool, Trouble},
    tools::fields,
};
use serde_json::json;
use std::path::{Path, PathBuf};

const PATH: &str = "The project's directory, as a whole path — or one relative \
to the project this session is already in.";

pub static TOOLS: [Tool; 2] = [
    Tool {
        name: "project_open",
        description: "Open a directory as a project in cydonia, making the \
            directory first if it is not there yet.",
        schema: |bound| fields(bound, &[("path", PATH)]),
        writes: true,
        call: open,
    },
    Tool {
        name: "project_close",
        description: "Close a project in cydonia: it leaves the sidebar and its \
            agents stop. Nothing on disk is touched.",
        schema: |bound| fields(bound, &[("path", PATH)]),
        writes: true,
        call: close,
    },
];

// ── the tools ────────────────────────────────────────────────────

fn open(args: Args<'_>) -> Outcome {
    let path = whole(&args, args.text("path")?)?;
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

fn close(args: Args<'_>) -> Outcome {
    let path = settled(whole(&args, args.text("path")?)?);
    if !rail::is_open(&path) {
        return Err(Trouble::Refused(match held() {
            None => format!(
                "{} is not open, and neither is anything else",
                path.display()
            ),
            Some(open) => format!("{} is not open — cydonia has {open}", path.display()),
        }));
    }
    rail::ask(Change::Close(path.clone()))?;
    Ok(Answer::said(format!("closed {}", path.display())).with(json!({ "path": path })))
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

/// The rail, for a refusal to name — a model that named the wrong directory
/// can see the right one without a second call.
fn held() -> Option<String> {
    let open = rail::open();
    (!open.is_empty()).then(|| {
        open.iter()
            .map(|path| path.display().to_string())
            .collect::<Vec<_>>()
            .join(", ")
    })
}
