//! Startup screens: pick what to run, or browse the registry to install.

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

/// What a picked row does.
pub enum Source {
    /// Ready to run: configured in settings, or already installed.
    Ready(settings::Agent),
    /// Open the registry browser.
    Browse,
}

pub struct Choice {
    pub label: String,
    pub detail: Option<String>,
    pub source: Source,
    /// Session to continue, for resume rows.
    pub previous: Option<String>,
}

/// Pick a launch. `None` means the user quit.
pub fn pick(choices: &[Choice]) -> Result<Option<usize>> {
    let rows: Vec<Row> = choices
        .iter()
        .map(|choice| Row {
            label: choice.label.clone(),
            detail: choice.detail.clone(),
        })
        .collect();
    choose(
        " cydonia — pick an agent ",
        &rows,
        "↑/↓ move · Enter select · q quit",
    )
}

/// Browse installable registry agents. `None` means the user backed out.
pub fn browse(agents: &[cydonia_registry::Agent]) -> Result<Option<usize>> {
    let rows: Vec<Row> = agents
        .iter()
        .map(|agent| Row {
            label: format!("{} {}", agent.name, agent.version),
            detail: agent.description.clone(),
        })
        .collect();
    choose(
        " install an agent ",
        &rows,
        "↑/↓ move · Enter install · Esc back",
    )
}

// ── Generic list screen ──────────────────────────────────────────

struct Row {
    label: String,
    detail: Option<String>,
}

struct State<'a> {
    rows: &'a [Row],
    title: &'a str,
    hint: &'a str,
    selected: usize,
    chosen: bool,
}

fn choose(title: &str, rows: &[Row], hint: &str) -> Result<Option<usize>> {
    if rows.is_empty() {
        anyhow::bail!("nothing to pick from");
    }
    let state = tui::run_app_with_state(
        || {
            Ok(State {
                rows,
                title,
                hint,
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
                    state.selected = (state.selected + 1).min(state.rows.len() - 1);
                }
                KeyCode::Home => state.selected = 0,
                KeyCode::End => state.selected = state.rows.len() - 1,
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
    Ok(state.chosen.then_some(state.selected))
}

fn draw(frame: &mut ratatui::Frame, state: &State) {
    let area = frame.area();
    // Leave room for the border, the hint, and its blank line.
    let visible = (area.height.saturating_sub(6) as usize).clamp(1, state.rows.len());
    // Window follows the selection: it sits still until the selection
    // would leave the bottom, then trails it one row at a time.
    let scroll = state.selected.saturating_sub(visible.saturating_sub(1));

    let width = state
        .rows
        .iter()
        .map(|row| {
            row.label.chars().count() + row.detail.as_ref().map_or(0, |d| d.chars().count() + 3)
        })
        .max()
        .unwrap_or(0)
        .clamp(40, 96) as u16
        + 6;
    let width = width.min(area.width);
    let height = (visible as u16 + 4).min(area.height);
    let rect = Rect::new(
        area.width.saturating_sub(width) / 2,
        area.height.saturating_sub(height) / 2,
        width,
        height,
    );

    let inner = width.saturating_sub(4) as usize;
    let mut lines = Vec::new();
    for (i, row) in state.rows.iter().enumerate().skip(scroll).take(visible) {
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
        let mut spans = vec![
            Span::styled(marker, style),
            Span::styled(row.label.clone(), style),
        ];
        // Details are a nicety: only shown when the row leaves space.
        if let Some(detail) = &row.detail {
            let used = 2 + row.label.chars().count();
            let room = inner.saturating_sub(used + 3);
            if room >= 12 {
                let detail: String = detail.chars().take(room).collect();
                spans.push(Span::styled(
                    format!(" — {detail}"),
                    Style::new().add_modifier(Modifier::DIM),
                ));
            }
        }
        lines.push(Line::from(spans));
    }
    lines.push(Line::raw(""));

    let more = state.rows.len().saturating_sub(visible);
    let hint = if more > 0 {
        format!("{} · +{more} more", state.hint)
    } else {
        state.hint.to_owned()
    };
    lines.push(Line::from(Span::styled(
        hint,
        Style::new().add_modifier(Modifier::DIM),
    )));

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(tui::border_focused())
        .title(state.title);
    frame.render_widget(Paragraph::new(lines).block(block), rect);
}
