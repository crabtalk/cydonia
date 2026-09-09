//! What the general section shows beside the version: the commit this binary
//! was built from.
//!
//! Asked here rather than at runtime because the app that ships has no
//! repository to ask — a bundle in `/Applications` is a binary and an icon —
//! so the answer is compiled in or it does not exist.

use std::process::Command;

fn main() {
    println!("cargo::rustc-env=CYDONIA_COMMIT={}", commit());
    // Cargo has no reason of its own to look at git, so without these the
    // stamp is whichever commit was checked out the last time something else
    // forced a rebuild. `--git-path` resolves them through the repository
    // itself, which is what keeps this right inside a worktree, where `.git`
    // is a file pointing somewhere else.
    for file in ["HEAD", "refs"] {
        if let Some(path) = git(&["rev-parse", "--git-path", file]) {
            println!("cargo::rerun-if-changed={path}");
        }
    }
}

/// `1a2b3c4`, and `1a2b3c4-dirty` where the tree has been edited since — the
/// suffix git's own `describe` uses, because a build with changes in it is not
/// the commit it names.
///
/// `unknown` where there is no repository at all: a build from a tarball —
/// `cargo install`, a vendored tree — is still a build, so this says what it
/// does not know rather than failing.
fn commit() -> String {
    let Some(short) = git(&["rev-parse", "--short=7", "HEAD"]) else {
        return "unknown".into();
    };
    match git(&["status", "--porcelain"]).is_none_or(|tree| tree.is_empty()) {
        true => short,
        false => format!("{short}-dirty"),
    }
}

/// One git command, or nothing at all: every caller here has an answer for a
/// machine with no git on it.
fn git(args: &[&str]) -> Option<String> {
    let out = Command::new("git").args(args).output().ok()?;
    if !out.status.success() {
        return None;
    }
    Some(String::from_utf8(out.stdout).ok()?.trim().to_owned())
}
