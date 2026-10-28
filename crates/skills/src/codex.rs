use std::{
    fs,
    path::{Path, PathBuf},
};
use toml_edit::{ArrayOfTables, DocumentMut, Item, Table, value};

/// Codex's `[[skills.config]]` entries in its `config.toml`. Each names a
/// skill by its `SKILL.md` path; `enabled = false` switches it off. Codex reads
/// the file at start.
pub(crate) struct Config {
    path: PathBuf,
    doc: DocumentMut,
}

impl Config {
    pub(crate) fn load(home: &Path) -> Self {
        let path = home.join("config.toml");
        let doc = fs::read_to_string(&path)
            .ok()
            .and_then(|text| text.parse().ok())
            .unwrap_or_default();
        Self { path, doc }
    }

    fn entries(&self) -> Option<&ArrayOfTables> {
        self.doc.get("skills")?.get("config")?.as_array_of_tables()
    }

    pub(crate) fn off(&self, skill: &Path) -> bool {
        self.entries().is_some_and(|entries| {
            entries.iter().any(|entry| {
                names(entry, skill) && entry.get("enabled").and_then(Item::as_bool) == Some(false)
            })
        })
    }

    /// Switching off adds an entry; switching on drops every entry naming the
    /// skill.
    pub(crate) fn set(&mut self, skill: &Path, on: bool) -> anyhow::Result<()> {
        let skills = self
            .doc
            .entry("skills")
            .or_insert_with(|| {
                let mut table = Table::new();
                table.set_implicit(true);
                Item::Table(table)
            })
            .as_table_mut()
            .ok_or_else(|| anyhow::anyhow!("skills in {} is not a table", self.path.display()))?;
        let entries = skills
            .entry("config")
            .or_insert_with(|| Item::ArrayOfTables(ArrayOfTables::new()))
            .as_array_of_tables_mut()
            .ok_or_else(|| {
                anyhow::anyhow!("skills.config in {} is not an array", self.path.display())
            })?;
        entries.retain(|entry| !names(entry, skill));
        if !on {
            let mut entry = Table::new();
            entry.insert("path", value(skill.display().to_string()));
            entry.insert("enabled", value(false));
            entries.push(entry);
        }
        if entries.is_empty() {
            skills.remove("config");
        }
        if skills.is_empty() {
            self.doc.remove("skills");
        }
        if let Some(dir) = self.path.parent() {
            fs::create_dir_all(dir)?;
        }
        fs::write(&self.path, self.doc.to_string())?;
        Ok(())
    }
}

/// Whether `entry` is about the skill at `skill`, its path as written or
/// through symlinks.
fn names(entry: &Table, skill: &Path) -> bool {
    let Some(path) = entry.get("path").and_then(Item::as_str).map(Path::new) else {
        return false;
    };
    path == skill
        || matches!(
            (path.canonicalize(), skill.canonicalize()),
            (Ok(a), Ok(b)) if a == b
        )
}
