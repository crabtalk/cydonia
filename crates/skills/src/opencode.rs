use crate::{Scope, json::JsonFile};
use serde_json::{Map, Value};
use std::path::{Path, PathBuf};

/// The permission a skill is asked for under, and the effects that deny and
/// allow it.
const SKILL: &str = "skill";
const DENY: &str = "deny";
const ALLOW: &str = "allow";

/// OpenCode's config files a scope reads, highest precedence first: for a
/// project, its `opencode.json`, then the global one; for a personal skill,
/// the global one alone. The first file is the one written.
///
/// A skill is denied by `"permission": {"skill": {"<pattern>": "deny"}}`. The
/// rules of every file are read lowest file first, each in order, and the last
/// one whose pattern matches the name decides. OpenCode 2.x also reads a
/// native `permissions` array; it is not read here.
pub(crate) struct Config {
    files: Vec<JsonFile>,
}

impl Config {
    pub(crate) fn load(home: &Path, scope: &Scope) -> Self {
        let mut paths = Vec::new();
        if let Scope::Project(project) = scope {
            paths.push(Self::file(project));
        }
        paths.push(Self::file(home));
        Self {
            files: paths.into_iter().map(JsonFile::load).collect(),
        }
    }

    /// `opencode.json` in `dir`, or `opencode.jsonc` where only that is there.
    fn file(dir: &Path) -> PathBuf {
        let jsonc = dir.join("opencode.jsonc");
        match jsonc.exists() && !dir.join("opencode.json").exists() {
            true => jsonc,
            false => dir.join("opencode.json"),
        }
    }

    /// The effect the last rule matching `name` gives it, in `files` from the
    /// lowest.
    fn effect(files: &[JsonFile], name: &str) -> Option<String> {
        let mut effect = None;
        for file in files.iter().rev() {
            match file.get(&["permission"]) {
                Some(Value::String(all)) => effect = Some(all.clone()),
                Some(Value::Object(permission)) => match permission.get(SKILL) {
                    Some(Value::String(all)) => effect = Some(all.clone()),
                    Some(Value::Object(rules)) => {
                        for (pattern, rule) in rules {
                            if let Some(rule) = rule.as_str()
                                && matches(pattern, name)
                            {
                                effect = Some(rule.to_owned());
                            }
                        }
                    }
                    _ => {}
                },
                _ => {}
            }
        }
        effect
    }

    pub(crate) fn off(&self, name: &str) -> bool {
        Self::effect(&self.files, name).as_deref() == Some(DENY)
    }

    /// Switching off writes the name's rule last; switching on drops it, and
    /// writes `allow` where another rule still denies it.
    pub(crate) fn set(&mut self, name: &str, on: bool) -> anyhow::Result<()> {
        let path = self.files[0].path.display().to_string();
        self.files[0].edit(|config| {
            let permission = config
                .entry("permission")
                .or_insert_with(|| Value::Object(Map::new()));
            let Value::Object(permission) = permission else {
                anyhow::bail!("permission in {path} is not an object");
            };
            let rules = permission
                .entry(SKILL)
                .or_insert_with(|| Value::Object(Map::new()));
            let Value::Object(rules) = rules else {
                anyhow::bail!("permission.skill in {path} is not an object");
            };
            rules.shift_remove(name);
            if !on {
                rules.insert(name.to_owned(), DENY.into());
            }
            Ok(())
        })?;
        if on && self.off(name) {
            self.files[0].edit(|config| {
                if let Some(Value::Object(rules)) = config
                    .get_mut("permission")
                    .and_then(|permission| permission.get_mut(SKILL))
                {
                    rules.insert(name.to_owned(), ALLOW.into());
                }
                Ok(())
            })?;
        }
        if let Some(Value::Object(rules)) = self.files[0].get(&["permission", SKILL])
            && rules.is_empty()
        {
            self.files[0].edit(|config| {
                if let Some(Value::Object(permission)) = config.get_mut("permission") {
                    permission.shift_remove(SKILL);
                    if permission.is_empty() {
                        config.shift_remove("permission");
                    }
                }
                Ok(())
            })?;
        }
        Ok(())
    }
}

/// Whether `pattern`, where `*` stands for any run of characters, matches
/// all of `name`.
fn matches(pattern: &str, name: &str) -> bool {
    let mut parts = pattern.split('*');
    let first = parts.next().unwrap_or_default();
    let Some(mut rest) = name.strip_prefix(first) else {
        return false;
    };
    let parts: Vec<&str> = parts.collect();
    let Some((last, middle)) = parts.split_last() else {
        return rest.is_empty();
    };
    for part in middle {
        match rest.find(part) {
            Some(at) => rest = &rest[at + part.len()..],
            None => return false,
        }
    }
    rest.ends_with(last)
}
