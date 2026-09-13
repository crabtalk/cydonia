//! Two things the binary cannot work out for itself: the commit it was built
//! from, and whether it is the build that ships.
//!
//! The commit is asked here rather than at runtime because the app that ships
//! has no repository to ask — a bundle in `/Applications` is a binary and an
//! icon — so the answer is compiled in or it does not exist.

use std::process::Command;

fn main() {
    println!("cargo::rustc-env=CYDONIA_COMMIT={}", commit());
    // Declared whatever the profile is, or every `cfg!(prod)` in the crate is
    // an unexpected-cfg warning — which CI turns into an error.
    println!("cargo::rustc-check-cfg=cfg(prod)");
    if profile().as_deref() == Some("prod") {
        println!("cargo::rustc-cfg=prod");
    }
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

/// Which profile is being built, which cargo names to a build script nowhere:
/// `PROFILE` is only ever `debug` or `release`, whatever the profile is called.
/// `OUT_DIR` does carry it — `target/<profile>/build/<pkg>-<hash>/out`, with the
/// target triple in front of it for a cross build — so the answer is the
/// component before `build`.
fn profile() -> Option<String> {
    let out = std::env::var("OUT_DIR").ok()?;
    let mut parts = std::path::Path::new(&out).components().rev();
    parts.find(|part| part.as_os_str() == "build")?;
    Some(parts.next()?.as_os_str().to_str()?.to_owned())
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
