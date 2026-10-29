use crate::{Scope, json::JsonFile};
use serde_json::{Map, Value};
use std::path::Path;

/// The settings key mapping a skill's name to how Claude Code shows it.
const KEY: &str = "skillOverrides";
/// The settings key mapping a plugin's id, `name@origin`, to whether it loads.
const PLUGINS: &str = "enabledPlugins";
/// The value that switches a skill off. `on`, `name-only` and
/// `user-invocable-only` leave it on.
const OFF: &str = "off";
const ON: &str = "on";

/// Claude Code's settings files a scope reads, highest precedence first: for a
/// project, its `.claude/settings.local.json`, its `.claude/settings.json`,
/// then the user's `settings.json`; for a personal skill, the user's alone.
/// The first file is the one written.
pub(crate) struct Settings {
    files: Vec<JsonFile>,
}

impl Settings {
    pub(crate) fn load(home: &Path, scope: &Scope) -> Self {
        let mut paths = Vec::new();
        if let Scope::Project(project) = scope {
            paths.push(project.join(".claude/settings.local.json"));
            paths.push(project.join(".claude/settings.json"));
        }
        paths.push(home.join("settings.json"));
        Self {
            files: paths.into_iter().map(JsonFile::load).collect(),
        }
    }

    /// What the first file setting `name` says of it, from `from` on.
    fn value(&self, from: usize, name: &str) -> Option<&str> {
        self.files[from..]
            .iter()
            .find_map(|file| file.get(&[KEY, name])?.as_str())
    }

    /// Whether the first file naming the plugin `id` turns it on.
    pub(crate) fn plugin(&self, id: &str) -> Option<bool> {
        self.files
            .iter()
            .find_map(|file| file.get(&[PLUGINS, id])?.as_bool())
    }

    pub(crate) fn off(&self, name: &str) -> bool {
        self.value(0, name) == Some(OFF)
    }

    /// Switching on drops the written file's entry, and writes `on` where a
    /// file below it still says `off`.
    pub(crate) fn set(&mut self, name: &str, on: bool) -> anyhow::Result<()> {
        let below = self.value(1, name) == Some(OFF);
        let file = &mut self.files[0];
        let path = file.path.display().to_string();
        file.edit(|settings| {
            let overrides = settings
                .entry(KEY)
                .or_insert_with(|| Value::Object(Map::new()));
            let Value::Object(overrides) = overrides else {
                anyhow::bail!("{KEY} in {path} is not an object");
            };
            match (on, below) {
                (false, _) => {
                    overrides.insert(name.to_owned(), OFF.into());
                }
                (true, true) => {
                    overrides.insert(name.to_owned(), ON.into());
                }
                (true, false) => {
                    overrides.shift_remove(name);
                }
            }
            if overrides.is_empty() {
                settings.shift_remove(KEY);
            }
            Ok(())
        })
    }
}
