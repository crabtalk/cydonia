//! The `/mcp` picker: toggle MCP servers, or add one from the registry.
//!
//! ACP fixes a session's MCP servers at `session/new`, so changes here
//! apply to the next session rather than the running one.

use crate::tui;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use cydonia_core::settings::{self, McpServer};
use cydonia_registry::mcp;
use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span},
};

/// Results of background work, delivered into the event loop.
pub enum McpEvent {
    Found(Result<Vec<mcp::Server>, String>),
    Added(Result<McpServer, String>),
}

/// What the modal is doing.
enum Mode {
    /// Listing the user's servers.
    List,
    /// Searching the registry.
    Search {
        query: String,
        results: Vec<mcp::Server>,
        selected: usize,
        busy: bool,
        error: Option<String>,
    },
}

pub struct McpPicker {
    servers: Vec<McpServer>,
    selected: usize,
    mode: Mode,
    /// Set once something changes, so we can say it applies next session.
    dirty: bool,
}

/// What the caller should do after a key.
pub enum McpAction {
    None,
    Close,
    /// Run a registry search off the event loop.
    Search(String),
    /// Install and add this server off the event loop.
    Add(Box<mcp::Server>),
    Notice(String),
}

impl McpPicker {
    pub fn open() -> Self {
        Self {
            servers: settings::mcp_servers(),
            selected: 0,
            mode: Mode::List,
            dirty: false,
        }
    }

    pub fn dirty(&self) -> bool {
        self.dirty
    }

    /// Rows in list mode: the servers plus the "add" row.
    fn rows(&self) -> usize {
        self.servers.len() + 1
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> McpAction {
        match &mut self.mode {
            Mode::List => self.list_key(key),
            Mode::Search { .. } => self.search_key(key),
        }
    }

    fn list_key(&mut self, key: KeyEvent) -> McpAction {
        match key.code {
            KeyCode::Esc => McpAction::Close,
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => McpAction::Close,
            KeyCode::Up | KeyCode::Char('k') => {
                self.selected = self.selected.saturating_sub(1);
                McpAction::None
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.selected = (self.selected + 1).min(self.rows().saturating_sub(1));
                McpAction::None
            }
            KeyCode::Char('d') if self.selected < self.servers.len() => {
                let removed = self.servers.remove(self.selected);
                self.selected = self.selected.min(self.rows().saturating_sub(1));
                self.dirty = true;
                self.save();
                McpAction::Notice(format!("removed {}", removed.name))
            }
            KeyCode::Enter | KeyCode::Char(' ') => {
                if self.selected >= self.servers.len() {
                    self.mode = Mode::Search {
                        query: String::new(),
                        results: Vec::new(),
                        selected: 0,
                        busy: true,
                        error: None,
                    };
                    return McpAction::Search(String::new());
                }
                let server = &mut self.servers[self.selected];
                server.enabled = !server.enabled;
                let (name, enabled) = (server.name.clone(), server.enabled);
                self.dirty = true;
                self.save();
                McpAction::Notice(format!(
                    "{name} {} — applies to the next session",
                    if enabled { "enabled" } else { "disabled" }
                ))
            }
            _ => McpAction::None,
        }
    }

    fn search_key(&mut self, key: KeyEvent) -> McpAction {
        let Mode::Search {
            query,
            results,
            selected,
            busy,
            ..
        } = &mut self.mode
        else {
            return McpAction::None;
        };
        match key.code {
            KeyCode::Esc => {
                self.mode = Mode::List;
                McpAction::None
            }
            KeyCode::Up => {
                *selected = selected.saturating_sub(1);
                McpAction::None
            }
            KeyCode::Down => {
                *selected = (*selected + 1).min(results.len().saturating_sub(1));
                McpAction::None
            }
            KeyCode::Enter => {
                let Some(server) = results.get(*selected) else {
                    return McpAction::None;
                };
                if !server.installable() {
                    return McpAction::Notice(format!("{} can't be launched yet", server.name));
                }
                *busy = true;
                McpAction::Add(Box::new(server.clone()))
            }
            KeyCode::Tab => {
                *busy = true;
                McpAction::Search(query.clone())
            }
            KeyCode::Backspace => {
                query.pop();
                McpAction::None
            }
            KeyCode::Char(c) => {
                query.push(c);
                McpAction::None
            }
            _ => McpAction::None,
        }
    }

    /// Fold in the result of background work.
    pub fn apply(&mut self, event: McpEvent) -> Option<String> {
        match event {
            McpEvent::Found(found) => {
                if let Mode::Search {
                    results,
                    selected,
                    busy,
                    error,
                    ..
                } = &mut self.mode
                {
                    *busy = false;
                    match found {
                        Ok(list) => {
                            *results = list;
                            *selected = 0;
                            *error = None;
                        }
                        Err(e) => *error = Some(e),
                    }
                }
                None
            }
            McpEvent::Added(added) => {
                if let Mode::Search { busy, error, .. } = &mut self.mode {
                    *busy = false;
                    if let Err(e) = &added {
                        *error = Some(e.clone());
                    }
                }
                match added {
                    Ok(server) => {
                        let name = server.name.clone();
                        self.servers.retain(|s| s.name != server.name);
                        self.servers.push(server);
                        self.dirty = true;
                        self.save();
                        self.mode = Mode::List;
                        self.selected = self.servers.len().saturating_sub(1);
                        Some(format!("added {name} — applies to the next session"))
                    }
                    Err(_) => None,
                }
            }
        }
    }

    fn save(&self) {
        let _ = settings::save_mcp_servers(&self.servers);
    }

    pub fn draw(&self, frame: &mut ratatui::Frame) {
        let rect = tui::centered(frame.area(), 76, 20);
        let inner = rect.width.saturating_sub(4) as usize;
        let body = rect.height.saturating_sub(4) as usize;

        let (title, lines) = match &self.mode {
            Mode::List => (" mcp servers ".to_owned(), self.list_lines(inner, body)),
            Mode::Search {
                query,
                results,
                selected,
                busy,
                error,
            } => (
                format!(" add mcp server — {query}_ "),
                search_lines(results, *selected, *busy, error.as_deref(), inner, body),
            ),
        };
        let hint = match self.mode {
            Mode::List => "↑/↓ move · space toggle · d remove · Esc close",
            Mode::Search { .. } => "type to filter · Tab search · Enter add · Esc back",
        };
        tui::modal(frame, rect, &title, lines, hint);
    }

    fn list_lines(&self, inner: usize, body: usize) -> Vec<Line<'static>> {
        if self.servers.is_empty() {
            return vec![
                Line::from(Span::styled(
                    "no mcp servers yet",
                    Style::new().add_modifier(Modifier::DIM),
                )),
                Line::raw(""),
                tui::row("+ add from the registry", "", true, inner),
            ];
        }
        let scroll = tui::window(self.selected, body.saturating_sub(1));
        let mut lines = Vec::new();
        for (i, server) in self.servers.iter().enumerate().skip(scroll).take(body - 1) {
            let mark = if server.enabled { "[x] " } else { "[ ] " };
            let label = format!("{mark}{}", server.name);
            lines.push(tui::row(
                &label,
                &server.detail(),
                i == self.selected,
                inner,
            ));
        }
        if self.selected >= self.servers.len() || self.servers.len() < body {
            lines.push(tui::row(
                "+ add from the registry",
                "",
                self.selected >= self.servers.len(),
                inner,
            ));
        }
        lines
    }
}

fn search_lines(
    results: &[mcp::Server],
    selected: usize,
    busy: bool,
    error: Option<&str>,
    inner: usize,
    body: usize,
) -> Vec<Line<'static>> {
    if let Some(error) = error {
        return vec![Line::from(Span::styled(
            tui::truncate(error, inner),
            Style::new().fg(Color::Indexed(204)),
        ))];
    }
    if busy {
        return vec![Line::from(Span::styled(
            "searching…",
            Style::new().add_modifier(Modifier::DIM),
        ))];
    }
    if results.is_empty() {
        return vec![Line::from(Span::styled(
            "no matches",
            Style::new().add_modifier(Modifier::DIM),
        ))];
    }
    let scroll = tui::window(selected, body.saturating_sub(1));
    results
        .iter()
        .enumerate()
        .skip(scroll)
        .take(body - 1)
        .map(|(i, server)| {
            let tag = if server.is_remote() { " (remote)" } else { "" };
            let label = format!("{}{tag}", server.name);
            let detail = if server.installable() {
                server.description.clone()
            } else {
                format!("unsupported — {}", server.description)
            };
            tui::row(&label, &detail, i == selected, inner)
        })
        .collect()
}

/// Turn a registry entry into a stored server, installing if needed.
pub fn install(data_dir: &std::path::Path, server: &mcp::Server) -> Result<McpServer, String> {
    let command = mcp::install(data_dir, server, |_| {}).map_err(|e| format!("{e:#}"))?;
    Ok(McpServer {
        name: server.name.clone(),
        enabled: true,
        command,
        args: Vec::new(),
        env: Default::default(),
        url: match &server.distribution {
            mcp::Distribution::Remote { url } => Some(url.clone()),
            _ => None,
        },
        id: Some(server.id.clone()),
    })
}
