//! Built-in resources, embedded at build time and shared by agent transports.
//!
//! Add `resources/<name>.md` with YAML `name` and `description` fields.
//! Rebuilding validates and bundles it automatically. Resource Markdown is embedded.

pub struct Resource {
    pub name: &'static str,
    pub description: &'static str,
    pub content: &'static str,
}

impl Resource {
    pub fn uri(&self) -> String {
        format!("cydonia://resources/{}", self.name)
    }
}

include!(concat!(env!("OUT_DIR"), "/resources.rs"));

pub fn list() -> &'static [Resource] {
    BUILTINS
}

pub fn read(name: &str) -> Option<&'static Resource> {
    BUILTINS.iter().find(|resource| resource.name == name)
}

/// Names and descriptions only; full instructions are loaded on demand.
/// Resources named in `hidden` are left out.
pub fn catalog(hidden: &[&str]) -> String {
    BUILTINS
        .iter()
        .filter(|resource| !hidden.contains(&resource.name))
        .map(|resource| {
            format!(
                "- {} ({}): {}",
                resource.name,
                resource.uri(),
                resource.description
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}
