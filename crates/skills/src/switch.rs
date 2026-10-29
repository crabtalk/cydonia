use crate::{Harness, Scope, claude, codex, gemini, opencode, pi};
use std::path::Path;

/// An agent's settings that switch its skills off, as read for one scope.
pub(crate) enum Switch {
    Claude(claude::Settings),
    Codex(codex::Config),
    Gemini(gemini::Settings),
    OpenCode(opencode::Config),
    Pi(pi::Settings),
}

impl Switch {
    /// `None` where the agent has no switch for a skill, or its home
    /// directory is unknown.
    pub(crate) fn load(harness: Harness, scope: &Scope) -> Option<Self> {
        let home = harness.home()?;
        match harness {
            Harness::Claude => Some(Self::Claude(claude::Settings::load(&home, scope))),
            Harness::Codex => Some(Self::Codex(codex::Config::load(&home))),
            Harness::Gemini => Some(Self::Gemini(gemini::Settings::load(&home, scope))),
            Harness::OpenCode => Some(Self::OpenCode(opencode::Config::load(&home, scope))),
            Harness::Pi => Some(Self::Pi(pi::Settings::load(&home, scope))),
            Harness::Cursor => None,
        }
    }

    /// Whether the skill `name` at `path` is switched off.
    pub(crate) fn off(&self, name: &str, path: &Path) -> bool {
        match self {
            Self::Claude(settings) => settings.off(name),
            Self::Codex(config) => config.off(path),
            Self::Gemini(settings) => settings.off(name),
            Self::OpenCode(config) => config.off(name),
            Self::Pi(settings) => settings.off(path),
        }
    }

    pub(crate) fn set(&mut self, name: &str, path: &Path, on: bool) -> anyhow::Result<()> {
        match self {
            Self::Claude(settings) => settings.set(name, on),
            Self::Codex(config) => config.set(path, on),
            Self::Gemini(settings) => settings.set(name, on),
            Self::OpenCode(config) => config.set(name, on),
            Self::Pi(settings) => settings.set(path, on),
        }
    }
}
