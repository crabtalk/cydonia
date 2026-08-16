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

/// One selectable launch: an agent, optionally continuing a previous
/// session.
#[derive(Clone)]
pub struct Choice {
    pub label: String,
    pub agent: settings::Agent,
    pub previous: Option<String>,
}

struct State<'a> {
    choices: &'a [Choice],
    selected: usize,
    chosen: bool,
}

/// Let the user pick a launch. Returns `None` if they quit instead.
pub fn pick(choices: &[Choice]) -> Result<Option<Choice>> {
    if choices.is_empty() {
        anyhow::bail!("no agents in settings.toml");
    }
    let state = tui::run_app_with_state(
        || {
            Ok(State {
                choices,
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
                    state.selected = (state.selected + 1).min(state.choices.len() - 1);
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
    Ok(state.chosen.then(|| state.choices[state.selected].clone()))
}

fn draw(frame: &mut ratatui::Frame, state: &State) {
    let area = frame.area();
    let height = (state.choices.len() as u16 + 4).min(area.height);
    let longest = state
        .choices
        .iter()
        .map(|c| c.label.chars().count() as u16)
        .max()
        .unwrap_or(0);
    let width = (longest + 8).max(44).min(area.width);
    let rect = Rect::new(
        area.width.saturating_sub(width) / 2,
        area.height.saturating_sub(height) / 2,
        width,
        height,
    );

    let mut lines = Vec::new();
    for (i, choice) in state.choices.iter().enumerate() {
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
            Span::styled(choice.label.clone(), style),
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
