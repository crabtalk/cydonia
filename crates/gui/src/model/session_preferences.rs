//! Personal session choices, scoped to a project and agent, outside the repository.

use super::settings::{self, Agent};
use anyhow::Result;
use bezel::gpui::{App, Global};
use cacp::schema::{
    SessionConfigKind, SessionConfigOption, SessionConfigOptionValue, SessionConfigSelectOption,
    SessionConfigSelectOptions, SessionModeState,
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

/// Put `value` on `option` as its current value. Callers check
/// [`supports`] first; a value of the other shape is left out.
pub fn adopt(option: &mut SessionConfigOption, value: SessionConfigOptionValue) {
    match (&mut option.kind, value) {
        (SessionConfigKind::Select(select), SessionConfigOptionValue::ValueId { value }) => {
            select.current_value = value;
        }
        (SessionConfigKind::Boolean(flag), SessionConfigOptionValue::Boolean { value }) => {
            flag.current_value = value;
        }
        _ => {}
    }
}

/// A select's values, with a group's rows folded in beside the ungrouped ones.
pub fn select_values(options: &SessionConfigSelectOptions) -> Vec<&SessionConfigSelectOption> {
    match options {
        SessionConfigSelectOptions::Ungrouped(options) => options.iter().collect(),
        SessionConfigSelectOptions::Grouped(groups) => groups
            .iter()
            .flat_map(|group| group.options.iter())
            .collect(),
    }
}

pub fn supports(option: &SessionConfigOption, value: &SessionConfigOptionValue) -> bool {
    match (&option.kind, value) {
        (SessionConfigKind::Boolean(_), SessionConfigOptionValue::Boolean { .. }) => true,
        (SessionConfigKind::Select(select), SessionConfigOptionValue::ValueId { value }) => {
            select_values(&select.options)
                .iter()
                .any(|option| &option.value == value)
        }
        _ => false,
    }
}

/// What an agent offered in a project the last time it connected: its modes
/// and config options, as `session/new` reported them, and the values they
/// were on. Read while no connection is live, so the menu can list them.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Offers {
    pub modes: Option<SessionModeState>,
    pub config: Vec<SessionConfigOption>,
}

impl Offers {
    /// These offers with `choices` on them, where the agent still offers them.
    pub fn with(&self, choices: &Choices) -> Self {
        let mut offers = self.clone();
        if let Some(mode) = choices.mode.as_deref()
            && let Some(modes) = offers.modes.as_mut()
            && modes.available_modes.iter().any(|m| &*m.id == mode)
        {
            modes.current_mode_id = mode.into();
        }
        for option in &mut offers.config {
            if let Some(value) = choices.config.get(&*option.id)
                && supports(option, value)
            {
                adopt(option, value.clone());
            }
        }
        offers
    }
}

impl Offers {
    fn is_empty(&self) -> bool {
        self.modes.is_none() && self.config.is_empty()
    }
}

/// Every agent's [`Offers`] per project, read from `session-preferences.json`
/// once and written through. Every composer and default reads offers here.
#[derive(Default)]
pub struct OfferCache(BTreeMap<String, BTreeMap<PathBuf, Offers>>);

impl Global for OfferCache {}

impl OfferCache {
    /// Read the cache in, once, at launch.
    pub fn load(cx: &mut App) {
        let mut cache = Self::default();
        for (cwd, agents) in path().and_then(|path| read(&path)).unwrap_or_default() {
            for (key, agent) in agents {
                if !agent.offers.is_empty() {
                    cache
                        .0
                        .entry(key)
                        .or_default()
                        .insert(cwd.clone(), agent.offers);
                }
            }
        }
        cx.set_global(cache);
    }

    /// What `agent` offers in the project at `cwd`: its offers there, else
    /// its offers in another project, else nothing.
    pub fn get(cx: &App, cwd: &Path, agent: &Agent) -> Offers {
        let Some(projects) = cx
            .try_global::<Self>()
            .and_then(|cache| cache.0.get(&agent_key(agent)))
        else {
            return Offers::default();
        };
        projects
            .get(cwd)
            .or_else(|| projects.values().next())
            .cloned()
            .unwrap_or_default()
    }

    /// Keep what `agent` offered on connecting in the project at `cwd`.
    pub fn remember(cx: &mut App, cwd: &Path, agent: &Agent, offers: Offers) -> Result<()> {
        edit(cwd, agent, |agent| agent.offers = offers.clone())?;
        cx.default_global::<Self>()
            .0
            .entry(agent_key(agent))
            .or_default()
            .insert(cwd.to_path_buf(), offers);
        Ok(())
    }
}

#[derive(Default, Serialize, Deserialize)]
#[serde(default)]
struct AgentChoices {
    defaults: Choices,
    sessions: BTreeMap<String, Choices>,
    offers: Offers,
}

type Stored = BTreeMap<PathBuf, BTreeMap<String, AgentChoices>>;

fn path() -> Result<PathBuf> {
    Ok(settings::dir()?.join("session-preferences.json"))
}

pub fn agent_key(agent: &Agent) -> String {
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
    artifact::fs::save(&path, after)?;
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

#[cfg(test)]
#[path = "../../tests/unit/session_offers.rs"]
mod offers_tests;
