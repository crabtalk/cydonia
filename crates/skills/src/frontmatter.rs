use serde::Deserialize;

/// The YAML block a `SKILL.md` opens with, as the Agent Skills spec has it.
/// Either field may be missing: an agent still loads such a skill.
#[derive(Clone, Debug, Default, Deserialize)]
pub struct Frontmatter {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
}

impl Frontmatter {
    /// The YAML between a leading `---` line and the next one, and the Markdown
    /// after it. `None` without a block. `source` is read with `\n` line ends.
    pub fn split(source: &str) -> Option<(&str, &str)> {
        source
            .strip_prefix("---\n")
            .and_then(|rest| rest.split_once("\n---\n"))
    }

    /// The block `source` opens with, and the Markdown after it. `None` without
    /// a block.
    pub fn parse(source: &str) -> Option<Result<(Self, &str), serde_yaml_ng::Error>> {
        let (yaml, body) = Self::split(source)?;
        Some(serde_yaml_ng::from_str(yaml).map(|meta| (meta, body)))
    }

    /// The description on one line, its runs of whitespace as single spaces.
    pub fn summary(&self) -> Option<String> {
        let description = self.description.as_deref()?;
        Some(description.split_whitespace().collect::<Vec<_>>().join(" "))
    }

    /// Whether `name` is one the spec allows: 1 to 64 lowercase letters,
    /// digits and single hyphens, neither first nor last.
    pub fn valid_name(name: &str) -> bool {
        !name.is_empty()
            && name.len() <= 64
            && name
                .bytes()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-')
            && !name.starts_with('-')
            && !name.ends_with('-')
            && !name.contains("--")
    }
}
