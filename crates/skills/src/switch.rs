use crate::{Harness, Scope, claude::Settings, codex::Config};
use std::path::Path;

/// An agent's settings that switch its skills off, as read for one scope.
pub(crate) enum Switch {
    Claude(Settings),
    Codex(Config),
}

impl Switch {
    /// `None` where the agent's home directory is unknown.
    pub(crate) fn load(harness: Harness, scope: &Scope) -> Option<Self> {
        let home = harness.home()?;
        Some(match harness {
            Harness::Claude => Self::Claude(Settings::load(&home, scope)),
            Harness::Codex => Self::Codex(Config::load(&home)),
        })
    }

    /// Whether the skill `name` at `path` is switched off.
    pub(crate) fn off(&self, name: &str, path: &Path) -> bool {
        match self {
            Self::Claude(settings) => settings.off(name),
            Self::Codex(config) => config.off(path),
        }
    }

    pub(crate) fn set(&mut self, name: &str, path: &Path, on: bool) -> anyhow::Result<()> {
        match self {
            Self::Claude(settings) => settings.set(name, on),
            Self::Codex(config) => config.set(path, on),
        }
    }
}
