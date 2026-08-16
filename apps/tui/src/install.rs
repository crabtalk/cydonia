//! Agent install screen — installs are visible, never silent, because
//! they hit the network and can fail.

use crate::tui;
use anyhow::Result;
use crossterm::event::{self, Event};
use cydonia_registry::{Agent, Installed};
use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
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
    let area = frame.area();
    let width = area.width.saturating_sub(4).min(100);
    let height = area.height.saturating_sub(4).min(24);
    let rect = Rect::new(
        area.width.saturating_sub(width) / 2,
        area.height.saturating_sub(height) / 2,
        width,
        height,
    );

    let body = height.saturating_sub(4) as usize;
    let mut rows: Vec<Line> = lines
        .iter()
        .rev()
        .take(body)
        .rev()
        .map(|line| {
            let text: String = line
                .chars()
                .take(width.saturating_sub(4) as usize)
                .collect();
            Line::from(Span::styled(text, Style::new().add_modifier(Modifier::DIM)))
        })
        .collect();

    rows.push(Line::raw(""));
    rows.push(match error {
        Some(error) => {
            let text: String = format!("failed: {error}")
                .chars()
                .take(width.saturating_sub(4) as usize)
                .collect();
            Line::from(Span::styled(text, Style::new().fg(Color::Indexed(204))))
        }
        None => Line::from(Span::styled(
            "installing…",
            Style::new().fg(Color::Rgb(215, 119, 87)),
        )),
    });
    if error.is_some() {
        rows.push(Line::from(Span::styled(
            "press any key to go back",
            Style::new().add_modifier(Modifier::DIM),
        )));
    }

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(tui::border_focused())
        .title(format!(" installing {} {} ", agent.name, agent.version));
    frame.render_widget(Paragraph::new(rows).block(block), rect);
}
