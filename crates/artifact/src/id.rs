//! What names an article, a board or a session, and keeps naming it.
//!
//! Minted as the millisecond it was made, which is also what the filesystem
//! backend names its file — so a project written before ids existed has one
//! for every entry already, in the name of the file it is in. Reading takes it
//! from there when the record itself carries none.
//!
//! It is a string rather than a number because it is only ever compared and
//! handed about: a backend that would rather mint a uuid or a rowid should be
//! able to, and nothing here reads the millisecond back out.

/// This millisecond. Sorting these is sorting by age, which is the order
/// entries are listed back in — and two made inside one millisecond is the
/// only collision, which each backend settles where it puts the entry.
pub fn mint() -> String {
    crate::stamp::now().to_string()
}
