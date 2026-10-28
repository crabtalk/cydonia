use crate::{Harness, Switch};
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
    pub enabled: bool,
}

impl Skill {
    /// Blocking: switch it on or off in its agent's own settings. Its files
    /// are left as they are.
    pub fn set_enabled(&self, on: bool) -> anyhow::Result<()> {
        let Some(mut switch) = Switch::load(self.harness, &self.scope) else {
            anyhow::bail!("no home directory for {:?}", self.harness);
        };
        switch.set(&self.name, &self.path, on)
    }
}
