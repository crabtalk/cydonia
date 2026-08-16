//! Startup screens: pick what to run, or browse the registry to install.

use crate::tui;
use anyhow::Result;
use crossterm::event::{KeyCode, KeyModifiers};
use cydonia_core::settings;

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
    let scroll = tui::window(state.selected, visible);

    // Unlike the other modals this one sizes itself to its content —
    // the launcher is short and should not span the terminal.
    let widest = state
        .rows
        .iter()
        .map(|row| {
            row.label.chars().count() + row.detail.as_ref().map_or(0, |d| d.chars().count() + 3)
        })
        .max()
        .unwrap_or(0)
        .clamp(40, 96) as u16;
    let rect = tui::centered(area, widest + 6, visible as u16 + 4);
    let inner = rect.width.saturating_sub(4) as usize;

    let lines = state
        .rows
        .iter()
        .enumerate()
        .skip(scroll)
        .take(visible)
        .map(|(i, row)| {
            tui::row(
                &row.label,
                row.detail.as_deref().unwrap_or_default(),
                i == state.selected,
                inner,
            )
        })
        .collect();

    let more = state.rows.len().saturating_sub(visible);
    let hint = if more > 0 {
        format!("{} · +{more} more", state.hint)
    } else {
        state.hint.to_owned()
    };
    tui::modal(frame, rect, state.title, lines, &hint);
}
