//! Personal session choices, scoped to a project and agent, outside the repository.

use super::settings::{self, Agent};
use anyhow::Result;
use cacp::schema::{
    SessionConfigKind, SessionConfigOption, SessionConfigOptionValue, SessionConfigSelectOptions,
    SessionModeState,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Choices {
    pub mode: Option<String>,
    pub config: BTreeMap<String, SessionConfigOptionValue>,
}

impl Choices {
    pub fn capture(modes: Option<&SessionModeState>, config: &[SessionConfigOption]) -> Self {
        Self {
            mode: modes.map(|modes| modes.current_mode_id.to_string()),
            config: config
                .iter()
                .map(|option| (option.id.to_string(), current(option)))
                .collect(),
        }
    }
}

pub fn current(option: &SessionConfigOption) -> SessionConfigOptionValue {
    match &option.kind {
        SessionConfigKind::Select(select) => SessionConfigOptionValue::ValueId {
            value: select.current_value.clone(),
        },
        SessionConfigKind::Boolean(flag) => SessionConfigOptionValue::Boolean {
            value: flag.current_value,
        },
    }
}

pub fn supports(option: &SessionConfigOption, value: &SessionConfigOptionValue) -> bool {
    match (&option.kind, value) {
        (SessionConfigKind::Boolean(_), SessionConfigOptionValue::Boolean { .. }) => true,
        (SessionConfigKind::Select(select), SessionConfigOptionValue::ValueId { value }) => {
            match &select.options {
                SessionConfigSelectOptions::Ungrouped(options) => {
                    options.iter().any(|option| &option.value == value)
                }
                SessionConfigSelectOptions::Grouped(groups) => groups
                    .iter()
                    .any(|group| group.options.iter().any(|option| &option.value == value)),
            }
        }
        _ => false,
    }
}

#[derive(Default, Serialize, Deserialize)]
#[serde(default)]
struct AgentChoices {
    defaults: Choices,
    sessions: BTreeMap<String, Choices>,
}

type Stored = BTreeMap<PathBuf, BTreeMap<String, AgentChoices>>;

fn path() -> Result<PathBuf> {
    Ok(settings::dir()?.join("session-preferences.json"))
}

fn agent_key(agent: &Agent) -> String {
    agent
        .id
        .as_ref()
        .map_or_else(|| format!("name:{}", agent.name), |id| format!("id:{id}"))
}

fn read(path: &Path) -> Result<Stored> {
    match std::fs::read(path) {
        Ok(bytes) => Ok(serde_json::from_slice(&bytes)?),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Stored::default()),
        Err(error) => Err(error.into()),
    }
}

pub fn load(cwd: &Path, agent: &Agent, session: Option<&str>) -> Choices {
    let stored = path().and_then(|path| read(&path)).unwrap_or_default();
    let Some(agent) = stored
        .get(cwd)
        .and_then(|project| project.get(&agent_key(agent)))
    else {
        return Choices::default();
    };
    session
        .and_then(|session| agent.sessions.get(session))
        .unwrap_or(&agent.defaults)
        .clone()
}

fn edit(cwd: &Path, agent: &Agent, change: impl FnOnce(&mut AgentChoices)) -> Result<()> {
    rewrite(|stored| {
        change(
            stored
                .entry(cwd.to_path_buf())
                .or_default()
                .entry(agent_key(agent))
                .or_default(),
        )
    })
}

fn rewrite(change: impl FnOnce(&mut Stored)) -> Result<()> {
    let path = path()?;
    let mut stored = read(&path)?;
    let before = serde_json::to_vec(&stored)?;
    change(&mut stored);
    let after = serde_json::to_vec(&stored)?;
    if before == after {
        return Ok(());
    }
    std::fs::create_dir_all(path.parent().expect("settings directory"))?;
    let temporary = path.with_extension("json.tmp");
    std::fs::write(&temporary, after)?;
    std::fs::rename(temporary, path)?;
    Ok(())
}

/// Drop the choices of session `id` in the project at `cwd`, under every agent.
pub fn forget_session(cwd: &Path, id: &str) -> Result<()> {
    keep_sessions_where(cwd, |session| session != id)
}

/// Drop the choices of every session in the project at `cwd` that `ids` does
/// not name. Each agent's defaults stay.
pub fn keep_sessions<'a>(cwd: &Path, ids: impl IntoIterator<Item = &'a str>) -> Result<()> {
    let ids: std::collections::BTreeSet<&str> = ids.into_iter().collect();
    keep_sessions_where(cwd, |session| ids.contains(session))
}

fn keep_sessions_where(cwd: &Path, keep: impl Fn(&str) -> bool) -> Result<()> {
    rewrite(|stored| {
        if let Some(project) = stored.get_mut(cwd) {
            for agent in project.values_mut() {
                agent.sessions.retain(|session, _| keep(session));
            }
        }
    })
}

pub fn remember_mode(cwd: &Path, agent: &Agent, mode: &str) -> Result<()> {
    edit(cwd, agent, |agent| agent.defaults.mode = Some(mode.into()))
}

pub fn remember_config(
    cwd: &Path,
    agent: &Agent,
    id: &str,
    value: SessionConfigOptionValue,
) -> Result<()> {
    edit(cwd, agent, |agent| {
        agent.defaults.config.insert(id.into(), value);
    })
}

pub fn remember_session(cwd: &Path, agent: &Agent, id: &str, choices: &Choices) -> Result<()> {
    edit(cwd, agent, |agent| {
        agent.sessions.insert(id.into(), choices.clone());
    })
}
