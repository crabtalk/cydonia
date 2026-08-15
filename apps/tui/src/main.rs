//! Cydonia — TUI client for ACP agents.

mod app;
mod chat;
mod input;
mod render;
mod select;
mod tui;

use cydonia_core::settings;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let settings = settings::load()?;
    let Some(agent) = select::pick(&settings.agents)? else {
        return Ok(());
    };
    app::run(agent).await
}
