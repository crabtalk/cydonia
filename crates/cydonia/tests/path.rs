//! What the login shell's answer is worth: reading a PATH out of it, and
//! what that PATH is joined with.

use cydonia::agent::path::{merge, parse};

/// The plain case — an `env` dump, one variable per line, in no order this
/// side chose.
#[test]
fn the_path_comes_out_of_an_env_dump() {
    let dump = "SHELL=/bin/zsh\nPATH=/opt/homebrew/bin:/usr/bin\nHOME=/Users/x\n";
    assert_eq!(parse(dump), Some("/opt/homebrew/bin:/usr/bin"));
}

/// A startup file that greets the terminal greets us too, and it does it
/// before `env` runs. Anything that looks like the answer but arrives first
/// is the banner, not the answer.
#[test]
fn a_banner_ahead_of_the_dump_does_not_win() {
    let dump = "welcome back\nPATH=/nonsense\nSHELL=/bin/zsh\nPATH=/opt/homebrew/bin\n";
    assert_eq!(parse(dump), Some("/opt/homebrew/bin"));
}

/// A shell that failed, or printed nothing we can use. The caller keeps the
/// PATH it was launched with.
#[test]
fn nothing_that_names_a_path_is_no_answer() {
    assert_eq!(parse(""), None);
    assert_eq!(parse("HOME=/Users/x\nMANPATH=/usr/share/man\n"), None);
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
