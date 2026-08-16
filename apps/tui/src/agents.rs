//! The `/agents` picker: install or remove ACP agents from the registry.
//!
//! Toggling an agent installs or removes it. Which agent a session
//! *talks to* is fixed when cydonia starts, so switching means
//! relaunching — the picker says so rather than pretending otherwise.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use cydonia_core::settings;
use cydonia_registry::{Agent, Installed};
use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
};
use std::path::Path;

/// Results of background work, delivered into the event loop.
pub enum AgentEvent {
    /// An install or removal settled: the notice, or the error.
    Changed(Result<String, String>),
}

/// One row: a registry agent, or an agent from settings.toml.
enum Row {
    Registry(Box<RegistryRow>),
    /// Hand-configured in settings.toml — shown for completeness,
    /// but not ours to install or remove.
    Configured {
        name: String,
        detail: String,
    },
}

struct RegistryRow {
    agent: Agent,
    installed: Option<Installed>,
}

pub struct AgentPicker {
    rows: Vec<Row>,
    selected: usize,
    busy: bool,
    dirty: bool,
    /// Set when the catalog isn't available (offline first run).
    unavailable: bool,
}

/// What the caller should do after a key.
pub enum AgentAction {
    None,
    Close,
    /// Install this agent off the event loop.
    Install(Box<Agent>),
    /// Remove this installed agent (id, display name).
    Remove(String, String),
    Notice(String),
}

impl AgentPicker {
    pub fn open(settings: &settings::Settings, data_dir: &Path) -> Self {
        // The cached catalog only: opening a modal must never block on
        // the network. The launcher already refreshed it at startup.
        let catalog = cydonia_registry::cached(data_dir);
        let unavailable = catalog.is_none();
        let mut rows: Vec<Row> = catalog
            .map(|registry| {
                registry
                    .agents
                    .into_iter()
                    .filter(|agent| agent.installable())
                    .map(|agent| {
                        let installed = cydonia_registry::installed(data_dir, &agent.id);
                        Row::Registry(Box::new(RegistryRow { agent, installed }))
                    })
                    .collect()
            })
            .unwrap_or_default();

        // Installed first, then the rest alphabetically — the list runs
        // to twenty-odd entries and yours should be on top.
        rows.sort_by_key(|row| match row {
            Row::Registry(row) => (row.installed.is_none(), row.agent.name.to_lowercase()),
            Row::Configured { name, .. } => (false, name.to_lowercase()),
        });
        rows.extend(settings.agents.iter().map(|agent| Row::Configured {
            name: agent.name.clone(),
            detail: format!("{} {}", agent.command, agent.args.join(" ")),
        }));

        Self {
            rows,
            selected: 0,
            busy: false,
            dirty: false,
            unavailable,
        }
    }

    pub fn dirty(&self) -> bool {
        self.dirty
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> AgentAction {
        match key.code {
            KeyCode::Esc => AgentAction::Close,
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                AgentAction::Close
            }
            KeyCode::Up | KeyCode::Char('k') => {
                self.selected = self.selected.saturating_sub(1);
                AgentAction::None
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.selected = (self.selected + 1).min(self.rows.len().saturating_sub(1));
                AgentAction::None
            }
            KeyCode::Enter | KeyCode::Char(' ') if !self.busy => self.toggle(),
            KeyCode::Char('u') if !self.busy => self.update(),
            _ => AgentAction::None,
        }
    }

    fn toggle(&mut self) -> AgentAction {
        match self.rows.get(self.selected) {
            Some(Row::Registry(row)) if row.installed.is_some() => {
                AgentAction::Remove(row.agent.id.clone(), row.agent.name.clone())
            }
            Some(Row::Registry(row)) => {
                self.busy = true;
                AgentAction::Install(Box::new(row.agent.clone()))
            }
            Some(Row::Configured { .. }) => {
                AgentAction::Notice("configured in settings.toml — edit it there".to_owned())
            }
            None => AgentAction::None,
        }
    }

    /// Reinstall an installed agent at the registry's pinned version.
    fn update(&mut self) -> AgentAction {
        match self.rows.get(self.selected) {
            Some(Row::Registry(row))
                if row
                    .installed
                    .as_ref()
                    .is_some_and(|i| i.version != row.agent.version) =>
            {
                self.busy = true;
                AgentAction::Install(Box::new(row.agent.clone()))
            }
            _ => AgentAction::None,
        }
    }

    /// Fold in the result of background work, refreshing install state.
    pub fn apply(&mut self, event: AgentEvent, data_dir: &Path) -> Option<String> {
        let AgentEvent::Changed(result) = event;
        self.busy = false;
        for row in &mut self.rows {
            if let Row::Registry(row) = row {
                row.installed = cydonia_registry::installed(data_dir, &row.agent.id);
            }
        }
        match result {
            Ok(notice) => {
                self.dirty = true;
                Some(notice)
            }
            Err(error) => Some(error),
        }
    }

    pub fn draw(&self, frame: &mut ratatui::Frame) {
        let area = frame.area();
        let width = area.width.saturating_sub(8).min(76);
        let height = area.height.saturating_sub(4).min(20);
        let rect = Rect::new(
            area.width.saturating_sub(width) / 2,
            area.height.saturating_sub(height) / 2,
            width,
            height,
        );
        let inner = width.saturating_sub(4) as usize;
        let body = height.saturating_sub(4) as usize;

        let mut lines = if self.unavailable {
            vec![Line::from(Span::styled(
                "registry unavailable — check your connection and restart",
                Style::new().add_modifier(Modifier::DIM),
            ))]
        } else {
            self.rows_lines(inner, body)
        };

        lines.push(Line::raw(""));
        lines.push(Line::from(Span::styled(
            if self.busy {
                "working..."
            } else {
                "↑/↓ move · space install/remove · u update · Esc close"
            },
            Style::new().add_modifier(Modifier::DIM),
        )));

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(crate::tui::border_focused())
            .title(" agents ");
        frame.render_widget(Clear, rect);
        frame.render_widget(Paragraph::new(lines).block(block), rect);
    }

    fn rows_lines(&self, inner: usize, body: usize) -> Vec<Line<'static>> {
        let visible = body.saturating_sub(1).max(1);
        let scroll = self.selected.saturating_sub(visible.saturating_sub(1));
        self.rows
            .iter()
            .enumerate()
            .skip(scroll)
            .take(visible)
            .map(|(i, row)| {
                let (label, detail) = match row {
                    Row::Registry(row) => {
                        let mark = if row.installed.is_some() {
                            "[x] "
                        } else {
                            "[ ] "
                        };
                        let detail = match &row.installed {
                            Some(installed) if installed.version != row.agent.version => {
                                format!("{} - update to {}", installed.version, row.agent.version)
                            }
                            Some(installed) => installed.version.clone(),
                            None => row
                                .agent
                                .description
                                .clone()
                                .unwrap_or_else(|| row.agent.version.clone()),
                        };
                        (format!("{mark}{}", row.agent.name), detail)
                    }
                    Row::Configured { name, detail } => (format!("(-) {name}"), detail.clone()),
                };
                row_line(&label, i == self.selected, &detail, inner)
            })
            .collect()
    }
}

fn row_line(label: &str, selected: bool, detail: &str, inner: usize) -> Line<'static> {
    let (marker, style) = if selected {
        (
            "> ",
            Style::new()
                .fg(Color::Rgb(215, 119, 87))
                .add_modifier(Modifier::BOLD),
        )
    } else {
        ("  ", Style::new().fg(Color::Gray))
    };
    let mut spans = vec![
        Span::styled(marker, style),
        Span::styled(label.to_owned(), style),
    ];
    if !detail.is_empty() {
        let used = 2 + label.chars().count();
        let room = inner.saturating_sub(used + 3);
        if room >= 10 {
            let detail: String = if detail.chars().count() > room {
                detail
                    .chars()
                    .take(room.saturating_sub(3))
                    .collect::<String>()
                    + "..."
            } else {
                detail.to_owned()
            };
            spans.push(Span::styled(
                format!(" — {detail}"),
                Style::new().add_modifier(Modifier::DIM),
            ));
        }
    }
    Line::from(spans)
}
