//! Moving what has landed in the wrong file into the right one.
//!
//! Every migration cydonia performs on its own config lives under here and
//! nowhere else. A carry-over hidden inside the reader that benefits from it
//! is a carry-over nobody can find to delete: it reads as ordinary parsing, it
//! is exercised by every launch forever, and the day the last old file is gone
//! there is no seam to cut along.
//!
//! # One module per release
//!
//! Named for the version that **performs** the migration, not the last one
//! that needs it — [`v0_1_4`] is what 0.1.4 does on first launch to a file
//! written by anything up to 0.1.3. Flyway's rule, and it is the one that
//! survives being read a year later: the module name is the tag it shipped in,
//! so the release notes and the code answer the same question. Naming it for
//! the old version instead means asking "has that one shipped yet?" every time
//! a file is added.
//!
//! A version needing two carry-overs puts both in its own module. Each module
//! is a `run` and whatever it needs, so retiring one is deleting a directory
//! entry, a line in [`run`], and its test file.
//!
//! # Retiring one
//!
//! Nothing here is free: every module runs on every launch forever. A
//! migration may be dropped once no supported upgrade path can still be
//! carrying the old file — in practice, once the version it fixes is far
//! enough back that a person on it would be told to reinstall rather than
//! update. Dropping one is not a fix anybody notices, so it needs to be
//! deliberate.

pub mod v0_1_11;
pub mod v0_1_4;

/// Run every migration, oldest first, before anything reads either file.
///
/// Called from `main` ahead of `settings::load` and `state::restore` — either
/// one reading first would read from before the move.
///
/// Best effort throughout. Nothing here is worth failing a launch over: a
/// carry-over that could not be written leaves both files as they were and is
/// tried again next launch.
pub fn run() {
    v0_1_4::run();
    v0_1_11::run();
}
