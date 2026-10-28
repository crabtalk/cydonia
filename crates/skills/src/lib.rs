//! Agent skills on disk: a directory holding a `SKILL.md`, as the Agent Skills
//! spec has it, and where each agent reads them from.

mod frontmatter;
mod harness;
mod skill;

pub use frontmatter::Frontmatter;
pub use harness::{Harness, Root};
pub use skill::{Scope, Skill};
