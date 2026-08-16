//! Agent install screen — installs are visible, never silent, because
//! they hit the network and can fail.

use crate::tui;
use anyhow::Result;
use crossterm::event::{self, Event};
use cydonia_registry::{Agent, Installed};
use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span},
};
use std::path::Path;

/// Install `agent`, showing live output. Returns `None` when the
/// install failed and the user acknowledged it (the caller goes back to
/// the picker).
pub fn run(agent: &Agent, data_dir: &Path) -> Result<Option<Installed>> {
    let mut terminal = tui::setup()?;
    let mut lines: Vec<String> = Vec::new();

    let result = {
        let terminal = &mut terminal;
        let lines = &mut lines;
        cydonia_registry::install(data_dir, agent, |line| {
            lines.push(line.to_owned());
            let _ = terminal.draw(|frame| draw(frame, agent, lines, None));
        })
    };

    let outcome = match &result {
        Ok(_) => None,
        Err(e) => Some(format!("{e:#}")),
    };
    terminal.draw(|frame| draw(frame, agent, &lines, outcome.as_deref()))?;

    // A failure is the whole point of showing this screen: hold it
    // until the user has read it.
    if result.is_err() {
        loop {
            if let Event::Key(_) = event::read()? {
                break;
            }
        }
    }
    tui::teardown(&mut terminal)?;
    Ok(result.ok())
}

fn draw(frame: &mut ratatui::Frame, agent: &Agent, lines: &[String], error: Option<&str>) {
    let rect = tui::centered(frame.area(), 100, 24);
    let inner = rect.width.saturating_sub(4) as usize;
    let body = rect.height.saturating_sub(5) as usize;

    // The tail of the installer's output, oldest first.
    let mut rows: Vec<Line> = lines
        .iter()
        .rev()
        .take(body)
        .rev()
        .map(|line| {
            Line::from(Span::styled(
                tui::truncate(line, inner),
                Style::new().add_modifier(Modifier::DIM),
            ))
        })
        .collect();

    rows.push(Line::raw(""));
    rows.push(match error {
        Some(error) => Line::from(Span::styled(
            tui::truncate(&format!("failed: {error}"), inner),
            Style::new().fg(Color::Indexed(204)),
        )),
        None => Line::from(Span::styled("installing...", Style::new().fg(tui::ACCENT))),
    });

    let hint = if error.is_some() {
        "press any key to go back"
    } else {
        ""
    };
    tui::modal(
        frame,
        rect,
        &format!(" installing {} {} ", agent.name, agent.version),
        rows,
        hint,
    );
}
