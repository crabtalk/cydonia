//! Built-in skills, embedded at build time and shared by agent transports.
//!
//! Add `skills/<name>/SKILL.md` with YAML `name` and `description` fields.
//! Rebuilding validates and bundles it automatically. Only SKILL.md is embedded.

pub struct Skill {
    pub name: &'static str,
    pub description: &'static str,
    pub content: &'static str,
}

include!(concat!(env!("OUT_DIR"), "/skills.rs"));

pub fn list() -> &'static [Skill] {
    BUILTINS
}

pub fn read(name: &str) -> Option<&'static Skill> {
    BUILTINS.iter().find(|skill| skill.name == name)
}

/// Names and descriptions only; full instructions are loaded on demand.
pub fn catalog() -> String {
    BUILTINS
        .iter()
        .map(|skill| format!("- {}: {}", skill.name, skill.description))
        .collect::<Vec<_>>()
        .join("\n")
}
