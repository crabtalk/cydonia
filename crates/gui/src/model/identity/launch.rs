use artifact::session::mode::Mode;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Creation {
    #[default]
    Gui,
    Terminal,
    Choose,
}

impl Creation {
    pub const ALL: [Self; 3] = [Self::Gui, Self::Terminal, Self::Choose];

    pub fn resolve(self, explicit: Option<Mode>) -> Mode {
        explicit.unwrap_or(match self {
            Self::Gui | Self::Choose => Mode::Gui,
            Self::Terminal => Mode::Terminal,
        })
    }
}

impl std::fmt::Display for Creation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Gui => "GUI",
            Self::Terminal => "Terminal",
            Self::Choose => "Choose on creation",
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Interactive {
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: BTreeMap<String, String>,
}
