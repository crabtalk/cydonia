use serde_json::{Map, Value};
use std::{fs, path::PathBuf};

/// A JSON settings file an agent keeps. A missing file reads as `{}`; one that
/// cannot be read or parsed is never written.
pub(crate) struct JsonFile {
    pub(crate) path: PathBuf,
    value: Result<Value, String>,
}

impl JsonFile {
    pub(crate) fn load(path: PathBuf) -> Self {
        let value = match fs::read_to_string(&path) {
            Ok(text) => serde_json::from_str(&text).map_err(|err| err.to_string()),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(Value::Object(Map::new())),
            Err(err) => Err(err.to_string()),
        };
        Self { path, value }
    }

    /// The value at `keys`, one object key after another.
    pub(crate) fn get(&self, keys: &[&str]) -> Option<&Value> {
        let mut value = self.value.as_ref().ok()?;
        for key in keys {
            value = value.get(key)?;
        }
        Some(value)
    }

    /// Change the top-level object and write the file back.
    pub(crate) fn edit(
        &mut self,
        change: impl FnOnce(&mut Map<String, Value>) -> anyhow::Result<()>,
    ) -> anyhow::Result<()> {
        let value = match &mut self.value {
            Ok(value) => value,
            Err(err) => anyhow::bail!("{} cannot be read: {err}", self.path.display()),
        };
        let Value::Object(object) = value else {
            anyhow::bail!("{} is not a JSON object", self.path.display());
        };
        change(object)?;
        if let Some(dir) = self.path.parent() {
            fs::create_dir_all(dir)?;
        }
        let mut text = serde_json::to_string_pretty(value)?;
        text.push('\n');
        fs::write(&self.path, text)?;
        Ok(())
    }
}
