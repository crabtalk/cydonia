//! Cydonia — TUI client for ACP agents.

mod app;
mod chat;
mod input;
mod select;
mod tui;

use cydonia_core::settings;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let settings = settings::load()?;
    let cwd = std::env::current_dir().unwrap_or_else(|_| "/".into());

    let mut choices = Vec::new();
    for agent in &settings.agents {
        choices.push(select::Choice {
            label: agent.name.clone(),
            agent: agent.clone(),
            previous: None,
        });
        if let Some(id) = settings::last_session(&agent.name, &cwd) {
            choices.push(select::Choice {
                label: format!("{} — continue last session", agent.name),
                agent: agent.clone(),
                previous: Some(id),
            });
        }
    }

    let Some(choice) = select::pick(&choices)? else {
        return Ok(());
    };
    app::run(choice.agent, choice.previous).await
}
