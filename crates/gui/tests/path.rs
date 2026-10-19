//! What the login shell's answer is worth: reading a PATH out of it, and
//! what that PATH is joined with.

use cydonia_gui::agent::path::{Environment, merge, resolve};
use std::{
    ffi::OsStr,
    path::{Path, PathBuf},
};

/// The plain case: a dump, one variable per entry, in no order this side
/// chose.
#[test]
fn the_path_comes_out_of_an_env_dump() {
    let dump = "\0SHELL=/bin/zsh\0PATH=/opt/homebrew/bin:/usr/bin\0HOME=/Users/x\0";
    let env = Environment::parse(dump);
    assert_eq!(env.get("PATH"), Some("/opt/homebrew/bin:/usr/bin"));
    assert_eq!(env.get("HOME"), Some("/Users/x"));
}

/// A value may hold `=` and line breaks; an entry whose text before the first
/// `=` is not a variable name sets nothing.
#[test]
fn only_variable_names_are_taken() {
    let dump = "\0OPTS=a=b\0hello world=1\09LIVES=x\0_UNDER=one\nTWO=two\0";
    let env = Environment::parse(dump);
    assert_eq!(env.get("OPTS"), Some("a=b"));
    assert_eq!(env.get("_UNDER"), Some("one\nTWO=two"));
    assert_eq!(env.iter().count(), 2);
}

/// A startup file that greets the terminal greets us too, and it does it
/// before the dump's first NUL. Anything that looks like the answer but
/// arrives first is the banner, not the answer.
#[test]
fn a_banner_ahead_of_the_dump_does_not_win() {
    let dump = "welcome back\nPATH=/nonsense\n\0SHELL=/bin/zsh\0PATH=/opt/homebrew/bin\0";
    assert_eq!(
        Environment::parse(dump).get("PATH"),
        Some("/opt/homebrew/bin")
    );
}

/// A shell that failed, or printed nothing we can use. The caller keeps the
/// PATH it was launched with.
#[test]
fn nothing_that_names_a_path_is_no_answer() {
    assert_eq!(Environment::parse("").get("PATH"), None);
    assert_eq!(Environment::parse("PATH=/usr/bin\n").get("PATH"), None);
    let dump = "\0HOME=/Users/x\0MANPATH=/usr/share/man\0";
    assert_eq!(Environment::parse(dump).get("PATH"), None);
}

/// The shell's answer leads, because it is the one with homebrew and nvm on
/// it. What launchd handed us follows, and the overlap is not repeated.
#[test]
fn the_shell_leads_and_the_inherited_path_follows() {
    assert_eq!(
        merge("/opt/homebrew/bin:/usr/bin:/bin", "/usr/bin:/bin:/usr/sbin"),
        "/opt/homebrew/bin:/usr/bin:/bin:/usr/sbin"
    );
}

/// A PATH somebody set on purpose survives being merged into, even when the
/// shell has never heard of what is on it.
#[test]
fn an_inherited_entry_the_shell_lacks_is_kept() {
    assert_eq!(
        merge("/usr/bin", "/usr/bin:/tmp/fake-toolchain"),
        "/usr/bin:/tmp/fake-toolchain"
    );
}

/// An empty entry is how a PATH says "the current directory" by accident —
/// a stray colon, usually. It is not carried across.
#[test]
fn empty_entries_are_dropped() {
    assert_eq!(merge("/usr/bin::/bin", ""), "/usr/bin:/bin");
    assert_eq!(merge("", "/usr/bin"), "/usr/bin");
}

/// Windows' launcher finds only `.exe` on the PATH, and npm links `.cmd` shims.
#[test]
fn a_bare_name_finds_its_shim_under_pathext() {
    let path = std::env::join_paths(["/a", "/b"]).unwrap();
    let found = resolve("npx", &path, ".COM;.EXE;.CMD", |p: &Path| {
        p == Path::new("/b/npx.cmd")
    });
    assert_eq!(found, Some(PathBuf::from("/b/npx.cmd")));
}

/// An installed agent's command is the extensionless `.bin` shim beside the
/// `.cmd` npm wrote for Windows.
#[test]
fn a_path_without_an_extension_finds_the_shim_beside_it() {
    let found = resolve("/x/.bin/agent", OsStr::new(""), ".EXE;.CMD", |p: &Path| {
        p == Path::new("/x/.bin/agent.cmd")
    });
    assert_eq!(found, Some(PathBuf::from("/x/.bin/agent.cmd")));
}

#[test]
fn a_name_with_an_extension_is_taken_as_written() {
    assert_eq!(
        resolve("node.exe", OsStr::new("/a"), ".EXE", |_: &Path| true),
        None
    );
}
