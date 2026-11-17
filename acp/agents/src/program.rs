use std::path::Path;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Program {
    Claude,
    Codex,
    Gemini,
    Cursor,
    OpenCode,
    Pi,
    Antigravity,
}

impl Program {
    pub const ALL: [Self; 7] = [
        Self::Claude,
        Self::Codex,
        Self::Gemini,
        Self::Cursor,
        Self::OpenCode,
        Self::Pi,
        Self::Antigravity,
    ];

    pub fn commands(self) -> &'static [&'static str] {
        match self {
            Self::Claude => &["claude"],
            Self::Codex => &["codex"],
            Self::Gemini => &["gemini"],
            Self::Cursor => &["agent", "cursor-agent"],
            Self::OpenCode => &["opencode"],
            Self::Pi => &["pi"],
            Self::Antigravity => &["agy"],
        }
    }

    pub fn recognizes_command(self, command: &Path) -> bool {
        command
            .file_stem()
            .and_then(|name| name.to_str())
            .is_some_and(|command| self.commands().contains(&command))
    }

    pub fn registry_ids(self) -> &'static [&'static str] {
        match self {
            Self::Claude => &["claude-acp"],
            Self::Codex => &["codex-acp"],
            Self::Gemini => &["gemini"],
            Self::Cursor => &["cursor"],
            Self::OpenCode => &["opencode", "opencode-v2"],
            Self::Pi => &["pi-acp"],
            Self::Antigravity => &["antigravity-acp"],
        }
    }

    pub fn package(self) -> Option<&'static str> {
        match self {
            Self::Claude => Some("@agentclientprotocol/claude-agent-acp"),
            Self::Codex => Some("@agentclientprotocol/codex-acp"),
            Self::Gemini => Some("@google/gemini-cli"),
            Self::Pi => Some("pi-acp"),
            Self::Cursor | Self::OpenCode | Self::Antigravity => None,
        }
    }
}
