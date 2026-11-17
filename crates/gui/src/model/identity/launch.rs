#[cfg(feature = "desktop")]
use crate::agent::mcp as agent_mcp;
use crate::model::{session_preferences::Choices, settings::Agent};
use artifact::session::mode::Mode;
use cydonia_acp::{agents::program::Program, proto::SessionConfigOptionValue};
#[cfg(feature = "desktop")]
use mcp::{Server, http};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, path::Path};

const MODEL_OPTION: &str = "model";
const REASONING_OPTION: &str = "reasoning_effort";
const MODEL_ARGUMENT: &str = "--model";
const MODEL_SHORT_ARGUMENT: &str = "-m";
const CONFIG_ARGUMENT: &str = "--config";
const ACP_ARGUMENTS: [&str; 3] = ["acp", "--acp", "--experimental-acp"];
#[cfg(feature = "desktop")]
const MCP_CONFIG_ARGUMENT: &str = "--mcp-config";
const INTERACTIVE_PROMPT_ARGUMENT: &str = "--prompt-interactive";
const INTERACTIVE_SHORT_PROMPT_ARGUMENT: &str = "-i";
const PROMPT_ARGUMENT: &str = "--prompt";
const ARGUMENTS_END: &str = "--";

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
    #[serde(default)]
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: BTreeMap<String, String>,
}

impl Interactive {
    #[cfg(feature = "desktop")]
    pub fn attach_mcp(&mut self, program: Program, url: &str, project: &Path) {
        let headers = BTreeMap::from([(http::PROJECT, http::encoded(project))]);
        let args = match program {
            Program::Claude => vec![
                MCP_CONFIG_ARGUMENT.to_owned(),
                serde_json::json!({
                    "mcpServers": {
                        (Server::NAME): { "type": "http", "url": url, "headers": headers }
                    }
                })
                .to_string(),
            ],
            Program::Codex => vec![
                CONFIG_ARGUMENT.to_owned(),
                format!(
                    "mcp_servers.{}.url={}",
                    Server::NAME,
                    toml::Value::String(url.to_owned())
                ),
                CONFIG_ARGUMENT.to_owned(),
                format!(
                    "mcp_servers.{}.http_headers.{}={}",
                    Server::NAME,
                    http::PROJECT,
                    toml::Value::String(http::encoded(project))
                ),
            ],
            _ => return,
        };
        self.args.splice(0..0, args);
        self.env.extend(
            agent_mcp::bypass_environment(&self.env).map(|(name, value)| (name.to_owned(), value)),
        );
    }

    pub fn prompt(&mut self, program: Option<Program>, prompt: String) {
        match program {
            Some(Program::Gemini) => self.args.push(INTERACTIVE_PROMPT_ARGUMENT.into()),
            Some(Program::Antigravity) => self.args.push(INTERACTIVE_SHORT_PROMPT_ARGUMENT.into()),
            Some(Program::OpenCode) => self.args.push(PROMPT_ARGUMENT.into()),
            _ => self.args.push(ARGUMENTS_END.into()),
        }
        self.args.push(prompt);
    }

    pub fn resolve(agent: &Agent, choices: &Choices, overrides: Option<&Self>) -> Option<Self> {
        let program = agent.program();
        let native = (matches!(
            program,
            Some(Program::Gemini | Program::Cursor | Program::OpenCode)
        ) || program
            .is_some_and(|program| program.recognizes_command(Path::new(&agent.command))))
            && !overrides.is_some_and(|launch| !launch.command.trim().is_empty());
        let command = overrides
            .map(|launch| launch.command.as_str())
            .filter(|command| !command.trim().is_empty())
            .or_else(|| {
                program.map(|program| {
                    if native {
                        agent.command.as_str()
                    } else {
                        program.commands()[0]
                    }
                })
            })?;
        let mut launch = Self {
            command: command.to_owned(),
            args: if native {
                agent
                    .args
                    .iter()
                    .filter(|arg| !ACP_ARGUMENTS.contains(&arg.as_str()))
                    .cloned()
                    .collect()
            } else {
                Vec::new()
            },
            env: if native {
                agent.env.clone()
            } else {
                BTreeMap::new()
            },
        };
        for (id, value) in &choices.config {
            let SessionConfigOptionValue::ValueId { value } = value else {
                continue;
            };
            match (program, id.as_str()) {
                (Some(_), MODEL_OPTION)
                    if !launch
                        .args
                        .iter()
                        .chain(overrides.into_iter().flat_map(|launch| &launch.args))
                        .any(|arg| {
                            arg == MODEL_ARGUMENT
                                || arg == MODEL_SHORT_ARGUMENT
                                || arg.starts_with("--model=")
                                || arg.starts_with("-m") && arg.len() > 2
                        }) =>
                {
                    launch
                        .args
                        .extend([MODEL_ARGUMENT.into(), value.to_string()]);
                }
                (Some(Program::Codex), REASONING_OPTION) => {
                    launch.args.extend([
                        CONFIG_ARGUMENT.into(),
                        format!(
                            "model_reasoning_effort={}",
                            toml::Value::String(value.to_string())
                        ),
                    ]);
                }
                _ => {}
            }
        }
        if let Some(overrides) = overrides {
            launch.args.extend(overrides.args.iter().cloned());
            launch.env.extend(overrides.env.clone());
        }
        Some(launch)
    }
}
