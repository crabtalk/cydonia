//! Read-only Git queries with literal, non-UTF-8 path support.

use anyhow::{Context as _, Result, bail};
use std::{
    ffi::OsString,
    io::Read as _,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

pub mod preview;

const PATCH_LIMIT: u64 = 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Area {
    Staged,
    Unstaged,
    Untracked,
}

impl Area {
    pub fn label(self) -> &'static str {
        match self {
            Self::Staged => "Staged",
            Self::Unstaged => "Unstaged",
            Self::Untracked => "Untracked",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Change {
    pub path: PathBuf,
    pub original: Option<PathBuf>,
    pub area: Area,
    pub status: char,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Repository {
    pub root: PathBuf,
    pub files: Vec<Change>,
}

fn command(root: &Path) -> Command {
    let mut git = Command::new("git");
    git.arg("--literal-pathspecs")
        .args(["-c", "core.fsmonitor=false"])
        .arg("-C")
        .arg(root)
        .env("GIT_OPTIONAL_LOCKS", "0")
        .env("LC_ALL", "C")
        .stdin(Stdio::null());
    git
}

fn os_path(bytes: &[u8]) -> PathBuf {
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStringExt as _;
        OsString::from_vec(bytes.to_vec()).into()
    }
    #[cfg(not(unix))]
    {
        OsString::from(String::from_utf8_lossy(bytes).into_owned()).into()
    }
}

pub fn status(cwd: &Path) -> Result<Option<Repository>> {
    let output = command(cwd)
        .args(["rev-parse", "--show-toplevel"])
        .output()
        .context("Could not run Git")?;
    if !output.status.success() {
        let message = String::from_utf8_lossy(&output.stderr);
        if message.contains("not a git repository") {
            return Ok(None);
        }
        bail!("{}", message.trim());
    }
    // Remove Git's terminator only: whitespace can be part of the root path.
    let root = os_path(output.stdout.strip_suffix(b"\n").unwrap_or(&output.stdout));
    let output = command(&root)
        .args([
            "status",
            "--porcelain=v1",
            "--renames",
            "-z",
            "--untracked-files=all",
            "--ignore-submodules=none",
        ])
        .output()?;
    if !output.status.success() {
        bail!("{}", String::from_utf8_lossy(&output.stderr).trim());
    }
    Ok(Some(Repository {
        root,
        files: parse_status(&output.stdout)?,
    }))
}

fn parse_status(bytes: &[u8]) -> Result<Vec<Change>> {
    let mut entries = bytes
        .split(|byte| *byte == 0)
        .filter(|entry| !entry.is_empty());
    let mut files = Vec::new();
    while let Some(entry) = entries.next() {
        if entry.len() < 4 || entry[2] != b' ' {
            bail!("Invalid Git status record");
        }
        let path = os_path(&entry[3..]);
        let original = if entry[..2].iter().any(|code| matches!(code, b'R' | b'C')) {
            Some(os_path(
                entries.next().context("Missing Git rename source")?,
            ))
        } else {
            None
        };
        if &entry[..2] == b"??" {
            files.push(Change {
                path,
                original,
                area: Area::Untracked,
                status: '?',
            });
            continue;
        }
        // Show conflicts once, in the working-tree section.
        if entry[..2].contains(&b'U') || matches!(&entry[..2], b"AA" | b"DD") {
            files.push(Change {
                path,
                original,
                area: Area::Unstaged,
                status: 'U',
            });
            continue;
        }
        for (code, area) in [(entry[0], Area::Staged), (entry[1], Area::Unstaged)] {
            if code != b' ' && code != b'!' {
                files.push(Change {
                    path: path.clone(),
                    original: original.clone(),
                    area,
                    status: code as char,
                });
            }
        }
    }
    files.sort_by_key(|file| {
        (
            match file.area {
                Area::Staged => 0,
                Area::Unstaged => 1,
                Area::Untracked => 2,
            },
            file.path.clone(),
        )
    });
    Ok(files)
}

pub fn diff(root: &Path, change: &Change) -> Result<String> {
    let mut git = command(root);
    git.args([
        "diff",
        "--no-ext-diff",
        "--no-textconv",
        "--no-color",
        "--submodule=short",
    ]);
    if change.area == Area::Untracked {
        // Let Git handle untracked file metadata; exit 1 means a diff.
        git.args([
            "--no-index",
            "--",
            if cfg!(windows) { "NUL" } else { "/dev/null" },
        ])
        .arg(&change.path);
    } else {
        if change.area == Area::Staged {
            git.arg("--cached");
        }
        git.arg("--").arg(&change.path);
        if let Some(original) = &change.original {
            git.arg(original);
        }
    }
    // Bound output before collecting large generated patches.
    let (exit, mut bytes, truncated) = output(git)?;
    let differs = change.area == Area::Untracked && exit.code() == Some(1);
    if !truncated && !exit.success() && !differs {
        bail!("Git could not read this diff. Refresh to try again.");
    }
    bytes.truncate(PATCH_LIMIT as usize);
    let mut patch = String::from_utf8_lossy(&bytes).into_owned();
    if truncated {
        patch.push_str("\nPreview truncated at 1 MiB.\n");
    }
    if patch.is_empty() {
        patch.push_str("No textual diff (the file may have changed since refresh).");
    }
    Ok(patch)
}

/// Bound patch and source reads, reporting truncation to callers.
fn output(mut git: Command) -> Result<(std::process::ExitStatus, Vec<u8>, bool)> {
    let mut child = git.stdout(Stdio::piped()).stderr(Stdio::null()).spawn()?;
    let mut bytes = Vec::new();
    let read = child
        .stdout
        .take()
        .context("Missing Git output")?
        .take(PATCH_LIMIT + 1)
        .read_to_end(&mut bytes);
    let truncated = bytes.len() as u64 > PATCH_LIMIT;
    if truncated || read.is_err() {
        let _ = child.kill();
    }
    let exit = child.wait()?;
    read?;
    Ok((exit, bytes, truncated))
}

#[cfg(all(test, unix))]
#[path = "../../tests/unit/git_status.rs"]
mod tests;
