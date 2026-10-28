use crate::Harness;
use std::path::PathBuf;

/// Whose a skill is: every project's on this machine, or one project's.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Scope {
    Personal,
    /// The project's directory, which the root is in or above.
    Project(PathBuf),
}

/// One skill an agent reads, as a scan found it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Skill {
    pub harness: Harness,
    /// The frontmatter's `name`, else the directory's.
    pub name: String,
    pub description: Option<String>,
    /// Its `SKILL.md`.
    pub path: PathBuf,
    /// The root it is under.
    pub root: PathBuf,
    pub scope: Scope,
}
