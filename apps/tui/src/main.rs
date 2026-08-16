//! Cydonia — TUI client for ACP agents.

mod agents;
mod app;
mod chat;
mod input;
mod install;
mod mcp;
mod select;
mod tui;

use anyhow::Result;
use cydonia_core::settings;
use select::{Choice, Source};
use std::collections::BTreeMap;
use std::path::Path;

#[tokio::main]
async fn main() -> Result<()> {
    let settings = settings::load()?;
    let cwd = std::env::current_dir().unwrap_or_else(|_| "/".into());
    let data_dir = settings::data_dir()?;
    // The catalog is a convenience: without it (offline, first run)
    // configured agents still launch.
    let catalog = cydonia_registry::catalog(&data_dir);

    loop {
        let choices = choices(&settings, &catalog, &data_dir, &cwd);
        let Some(picked) = select::pick(&choices)? else {
            return Ok(());
        };
        let choice = &choices[picked];
        let previous = choice.previous.clone();

        let agent = match &choice.source {
            Source::Ready(agent) => agent.clone(),
            Source::Browse => {
                let Some(registry) = &catalog else { continue };
                let installable: Vec<_> = registry
                    .agents
                    .iter()
                    .filter(|agent| {
                        agent.installable()
                            && cydonia_registry::installed(&data_dir, &agent.id).is_none()
                    })
                    .cloned()
                    .collect();
                let Some(picked) = select::browse(&installable)? else {
                    continue;
                };
                let agent = &installable[picked];
                match install::run(agent, &data_dir)? {
                    Some(installed) => agent_from(&agent.name, &installed),
                    None => continue,
                }
            }
        };
        return app::run(agent, previous).await;
    }
}

/// Everything launchable: configured agents, installed registry agents,
/// each with a resume row when a session exists, then the browser.
fn choices(
    settings: &settings::Settings,
    catalog: &Option<cydonia_registry::Registry>,
    data_dir: &Path,
    cwd: &Path,
) -> Vec<Choice> {
    let mut ready: Vec<(String, Option<String>, settings::Agent)> = settings
        .agents
        .iter()
        .map(|agent| (agent.name.clone(), None, agent.clone()))
        .collect();

    if let Some(registry) = catalog {
        for agent in &registry.agents {
            let Some(installed) = cydonia_registry::installed(data_dir, &agent.id) else {
                continue;
            };
            let detail = (installed.version != agent.version)
                .then(|| format!("update available: {}", agent.version));
            ready.push((
                agent.name.clone(),
                detail,
                agent_from(&agent.name, &installed),
            ));
        }
    }

    let mut choices = Vec::new();
    for (name, detail, agent) in ready {
        let previous = settings::last_session(&name, cwd);
        choices.push(Choice {
            label: name.clone(),
            detail,
            source: Source::Ready(agent.clone()),
            previous: None,
        });
        if let Some(id) = previous {
            choices.push(Choice {
                label: format!("{name} — continue last session"),
                detail: None,
                source: Source::Ready(agent),
                previous: Some(id),
            });
        }
    }

    if let Some(registry) = catalog {
        choices.push(Choice {
            label: "install an agent...".to_owned(),
            detail: Some(format!("{} in the ACP registry", registry.agents.len())),
            source: Source::Browse,
            previous: None,
        });
    }
    choices
}

fn agent_from(name: &str, installed: &cydonia_registry::Installed) -> settings::Agent {
    settings::Agent {
        name: name.to_owned(),
        command: installed.command.clone(),
        args: installed.args.clone(),
        env: BTreeMap::new(),
        mcp_servers: Vec::new(),
    }
}
