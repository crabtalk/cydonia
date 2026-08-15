//! Startup agent selector — a small centered list, no typing required.

use crate::tui;
use anyhow::Result;
use crossterm::event::{KeyCode, KeyModifiers};
use cydonia_core::settings;
use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

struct State<'a> {
    agents: &'a [settings::Agent],
    selected: usize,
    chosen: bool,
}

/// Let the user pick an agent. Returns `None` if they quit instead.
pub fn pick(agents: &[settings::Agent]) -> Result<Option<settings::Agent>> {
    if agents.is_empty() {
        anyhow::bail!("no agents in settings.toml");
    }
    let state = tui::run_app_with_state(
        || {
            Ok(State {
                agents,
                selected: 0,
                chosen: false,
            })
        },
        draw,
        |key, state| {
            match key.code {
                KeyCode::Up | KeyCode::Char('k') => {
                    state.selected = state.selected.saturating_sub(1);
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    state.selected = (state.selected + 1).min(state.agents.len() - 1);
                }
                KeyCode::Enter => {
                    state.chosen = true;
                    return Ok(Some(Ok(())));
                }
                KeyCode::Esc | KeyCode::Char('q') => return Ok(Some(Ok(()))),
                KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    return Ok(Some(Ok(())));
                }
                _ => {}
            }
            Ok(None)
        },
    )?;
    Ok(state.chosen.then(|| state.agents[state.selected].clone()))
}

fn draw(frame: &mut ratatui::Frame, state: &State) {
    let area = frame.area();
    let height = (state.agents.len() as u16 + 4).min(area.height);
    let width = 44.min(area.width);
    let rect = Rect::new(
        area.width.saturating_sub(width) / 2,
        area.height.saturating_sub(height) / 2,
        width,
        height,
    );

    let mut lines = Vec::new();
    for (i, agent) in state.agents.iter().enumerate() {
        let (marker, style) = if i == state.selected {
            (
                "> ",
                Style::new()
                    .fg(Color::Rgb(215, 119, 87))
                    .add_modifier(Modifier::BOLD),
            )
        } else {
            ("  ", Style::new().fg(Color::DarkGray))
        };
        lines.push(Line::from(vec![
            Span::styled(marker, style),
            Span::styled(agent.name.clone(), style),
        ]));
    }
    lines.push(Line::raw(""));
    lines.push(Line::from(Span::styled(
        "↑/↓ move · Enter select · q quit",
        Style::new().add_modifier(Modifier::DIM),
    )));

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(tui::border_focused())
        .title(" cydonia — pick an agent ");
    frame.render_widget(Paragraph::new(lines).block(block), rect);
}
