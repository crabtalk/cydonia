use crate::{Scope, json::JsonFile};
use serde_json::{Map, Value};
use std::path::Path;

/// The settings key listing the names of the skills switched off.
const DISABLED: [&str; 2] = ["skills", "disabled"];

/// Gemini CLI's settings files a scope reads: for a project, its
/// `.gemini/settings.json` then the user's `settings.json`; for a personal
/// skill, the user's alone. The first file is the one written. A skill is off
/// where any of them lists it.
pub(crate) struct Settings {
    files: Vec<JsonFile>,
}

impl Settings {
    pub(crate) fn load(home: &Path, scope: &Scope) -> Self {
        let mut paths = Vec::new();
        if let Scope::Project(project) = scope {
            paths.push(project.join(".gemini/settings.json"));
        }
        paths.push(home.join("settings.json"));
        Self {
            files: paths.into_iter().map(JsonFile::load).collect(),
        }
    }

    fn lists(file: &JsonFile, name: &str) -> bool {
        file.get(&DISABLED)
            .and_then(Value::as_array)
            .is_some_and(|names| names.iter().any(|listed| listed.as_str() == Some(name)))
    }

    pub(crate) fn off(&self, name: &str) -> bool {
        self.files.iter().any(|file| Self::lists(file, name))
    }

    /// Switching off lists it in the first file; switching on takes it out of
    /// every file that lists it, as `gemini skills enable` does.
    pub(crate) fn set(&mut self, name: &str, on: bool) -> anyhow::Result<()> {
        if !on {
            let file = &mut self.files[0];
            let path = file.path.display().to_string();
            return file.edit(|settings| {
                let skills = settings
                    .entry(DISABLED[0])
                    .or_insert_with(|| Value::Object(Map::new()));
                let Value::Object(skills) = skills else {
                    anyhow::bail!("skills in {path} is not an object");
                };
                let disabled = skills
                    .entry(DISABLED[1])
                    .or_insert_with(|| Value::Array(Vec::new()));
                let Value::Array(disabled) = disabled else {
                    anyhow::bail!("skills.disabled in {path} is not an array");
                };
                if !disabled.iter().any(|listed| listed.as_str() == Some(name)) {
                    disabled.push(name.into());
                }
                Ok(())
            });
        }
        for file in &mut self.files {
            if !Self::lists(file, name) {
                continue;
            }
            file.edit(|settings| {
                if let Some(Value::Array(disabled)) = settings
                    .get_mut(DISABLED[0])
                    .and_then(|skills| skills.get_mut(DISABLED[1]))
                {
                    disabled.retain(|listed| listed.as_str() != Some(name));
                }
                Ok(())
            })?;
        }
        Ok(())
    }
}
