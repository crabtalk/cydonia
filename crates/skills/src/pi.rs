use crate::{Scope, json::JsonFile};
use serde_json::Value;
use std::path::Path;

/// The settings key listing skill paths and patterns; an entry `-<path>`
/// switches off the skill at that exact path.
const SKILLS: &str = "skills";

/// pi's settings file for a scope: the user's `settings.json` for a personal
/// skill, the project's `.pi/settings.json` for one of its own. pi reads a
/// skill's directory or its `SKILL.md`, absolute, as its exact path.
pub(crate) struct Settings {
    file: JsonFile,
}

impl Settings {
    pub(crate) fn load(home: &Path, scope: &Scope) -> Self {
        let path = match scope {
            Scope::Personal => home.join("settings.json"),
            Scope::Project(project) => project.join(".pi/settings.json"),
        };
        Self {
            file: JsonFile::load(path),
        }
    }

    /// Whether `entry` is the exclusion of the skill at `skill`, its `SKILL.md`.
    fn excludes(entry: &Value, skill: &Path) -> bool {
        let Some(path) = entry.as_str().and_then(|entry| entry.strip_prefix('-')) else {
            return false;
        };
        let path = Path::new(path);
        path == skill || Some(path) == skill.parent()
    }

    pub(crate) fn off(&self, skill: &Path) -> bool {
        self.file
            .get(&[SKILLS])
            .and_then(Value::as_array)
            .is_some_and(|entries| entries.iter().any(|entry| Self::excludes(entry, skill)))
    }

    /// Switching off adds `-<directory>`; switching on drops every entry
    /// excluding it.
    pub(crate) fn set(&mut self, skill: &Path, on: bool) -> anyhow::Result<()> {
        let path = self.file.path.display().to_string();
        self.file.edit(|settings| {
            let entries = settings
                .entry(SKILLS)
                .or_insert_with(|| Value::Array(Vec::new()));
            let Value::Array(entries) = entries else {
                anyhow::bail!("skills in {path} is not an array");
            };
            entries.retain(|entry| !Self::excludes(entry, skill));
            if !on && let Some(dir) = skill.parent() {
                entries.push(format!("-{}", dir.display()).into());
            }
            if entries.is_empty() {
                settings.shift_remove(SKILLS);
            }
            Ok(())
        })
    }
}
