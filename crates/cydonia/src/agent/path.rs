//! The `PATH` a GUI launch does not get.
//!
//! Agents are named by command rather than by path — `npx`, or an installed
//! shim whose first line is `#!/usr/bin/env node` — and
//! [`super::acp::Session::spawn`] execs that name against this process's
//! environment. Started from a terminal that environment is the shell's and
//! every one of them resolves. Started from Finder, the Dock, or `open`,
//! launchd hands the app `/usr/bin:/bin:/usr/sbin:/sbin` and nothing else: no
//! homebrew, no nvm, no cargo. The agent that runs all through development
//! cannot start in the bundle, and neither can anything its own tools reach
//! for afterwards, because the child inherits whatever we were handed.
//!
//! So ask the login shell what the PATH is, once, before anything is spawned.

use std::{
    ffi::OsStr,
    process::{Command, Stdio},
    time::{Duration, Instant},
};

/// How long the shell is given to answer. A shell still thinking after this
/// is one whose startup files are doing something that cannot be waited on at
/// launch — the inherited PATH is the answer then, which is the answer an app
/// without any of this has.
const TIMEOUT: Duration = Duration::from_secs(5);

/// How often it is looked in on while it runs.
const TICK: Duration = Duration::from_millis(20);

/// Put the login shell's `PATH` on this process.
///
/// Call from `main` while it is still the only thread. Writing the
/// environment is unsound the moment a second one might be reading it, and
/// every reader in this app — every agent spawn, every `npm` the installer
/// runs — comes later.
///
/// Silent on every failure, because each one leaves the PATH we were launched
/// with and that is no worse than not having asked.
pub fn adopt() {
    // A terminal launch is already carrying the shell's PATH, and `TERM` is
    // how it says so: launchd sets neither, a terminal sets both. Nothing to
    // fix, and asking would cost a shell startup on every `cargo run`.
    if std::env::var_os("TERM").is_some() {
        return;
    }
    let Some(shell) = std::env::var_os("SHELL") else {
        return;
    };
    let Some(found) = query(&shell) else {
        return;
    };
    let merged = merge(&found, &std::env::var("PATH").unwrap_or_default());
    // SAFETY: the doc comment above is the contract — this runs from `main`
    // before the app has a second thread.
    unsafe { std::env::set_var("PATH", merged) };
}

/// Run the shell the way a terminal would and take the PATH out of what it
/// ends up with. Login *and* interactive: `.zshrc` is where nvm and the rest
/// install themselves, and a login shell on its own never reads it.
///
/// `env` rather than `echo $PATH` so the answer survives the shell it came
/// from. fish holds `PATH` as a list and would hand back a space-separated
/// line; the exported variable `env` prints is colon-separated in every shell
/// there is.
fn query(shell: &OsStr) -> Option<String> {
    let mut child = Command::new(shell)
        .args(["-l", "-i", "-c", "/usr/bin/env"])
        // An interactive shell offered a terminal on stdin would wait for one.
        .stdin(Stdio::null())
        // Whatever its startup files want to complain about is not ours to print.
        .stderr(Stdio::null())
        .stdout(Stdio::piped())
        .spawn()
        .ok()?;

    let deadline = Instant::now() + TIMEOUT;
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Err(_) => return None,
            Ok(None) if Instant::now() >= deadline => {
                let _ = child.kill();
                return None;
            }
            Ok(None) => std::thread::sleep(TICK),
        }
    }

    let output = child.wait_with_output().ok()?;
    parse(&String::from_utf8_lossy(&output.stdout)).map(str::to_owned)
}

/// The `PATH=` line of an `env` dump — the last one, because a startup file
/// that prints a banner prints it before `env` ever runs.
pub fn parse(env: &str) -> Option<&str> {
    env.lines()
        .rev()
        .find_map(|line| line.strip_prefix("PATH="))
}

/// The shell's PATH, then anything the inherited one had that it did not.
///
/// A merge and not a replacement: this only ever runs on a launch with
/// nothing to lose, but a PATH somebody put there on purpose — a wrapper
/// script, a test harness — is still not ours to drop.
pub fn merge(shell: &str, inherited: &str) -> String {
    let mut kept: Vec<&str> = Vec::new();
    for entry in shell.split(':').chain(inherited.split(':')) {
        if !entry.is_empty() && !kept.contains(&entry) {
            kept.push(entry);
        }
    }
    kept.join(":")
}
