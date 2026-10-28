//! Agent skills on disk: a directory holding a `SKILL.md`, as the Agent Skills
//! spec has it. Where each agent reads them from, and the setting each agent
//! switches one off with.

mod claude;
mod codex;
mod frontmatter;
mod harness;
mod skill;
mod switch;

pub use frontmatter::Frontmatter;
pub use harness::{Harness, Root};
pub use skill::{Scope, Skill};
use switch::Switch;
