//! Chat buffer — structured storage for streaming chat output.
//!
//! Entries are appended by the [`super::render::MarkdownRenderer`] and
//! flattened into `Vec<Line>` for display in the chat area widget.

use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span},
};
use termimad::{CompositeKind, FmtLine, FmtText, MadSkin};

// ── Brand colours (same palette as the old renderer) ─────────────

pub const GREEN: Color = Color::Indexed(71);
pub const RED: Color = Color::Indexed(204);
pub const SUBTLE: Color = Color::Indexed(240);

pub const S_DIM: Style = Style::new().add_modifier(Modifier::DIM);
pub const S_SUBTLE: Style = Style::new().fg(SUBTLE);

// ── Data model ───────────────────────────────────────────────────

/// Status of a tool invocation (drives the marker colour).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolStatus {
    Running,
    Success,
    Failure,
}

/// Status of one agent plan entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlanStatus {
    Pending,
    Active,
    Done,
}

/// One row of the agent's plan checklist.
#[derive(Debug, Clone)]
pub struct PlanRow {
    pub text: String,
    pub status: PlanStatus,
}

/// One logical chunk in the chat output.
#[derive(Debug, Clone)]
pub enum ChatEntry {
    /// Rendered markdown text (one or more display lines).
    Text(Vec<Line<'static>>),
    /// Tool call marker (`⏺ Title`), keyed by the ACP tool call id so
    /// status updates land on the right marker.
    ToolMarker {
        id: String,
        label: String,
        status: ToolStatus,
    },
    /// Tool result output (`⎿ ...`) attached to the tool call `id`.
    ToolResult {
        id: String,
        lines: Vec<Line<'static>>,
    },
    /// Thinking / reasoning text (dimmed, italic).
    Thinking(Vec<Line<'static>>),
    /// The agent's plan checklist.
    Plan(Vec<PlanRow>),
    /// Blank separator line.
    Blank,
}

/// Append-only buffer of chat entries.
#[derive(Debug, Default)]
pub struct ChatBuffer {
    pub entries: Vec<ChatEntry>,
}

impl ChatBuffer {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, entry: ChatEntry) {
        self.entries.push(entry);
    }

    /// Set the status of the tool marker with the given call id.
    pub fn set_tool_status(&mut self, id: &str, status: ToolStatus) {
        for entry in self.entries.iter_mut().rev() {
            if let ChatEntry::ToolMarker {
                id: mid, status: s, ..
            } = entry
                && mid == id
            {
                *s = status;
                return;
            }
        }
    }

    /// Update the label of the tool marker with the given call id.
    pub fn set_tool_label(&mut self, id: &str, label: String) {
        for entry in self.entries.iter_mut().rev() {
            if let ChatEntry::ToolMarker {
                id: mid, label: l, ..
            } = entry
                && mid == id
            {
                *l = label;
                return;
            }
        }
    }

    /// Attach result lines under the marker with the given call id, after
    /// any results already attached to it. Falls back to appending at the
    /// end if the marker is gone (e.g. after /clear).
    pub fn insert_tool_result(&mut self, id: &str, lines: Vec<Line<'static>>) {
        let marker = self
            .entries
            .iter()
            .position(|e| matches!(e, ChatEntry::ToolMarker { id: mid, .. } if mid == id));
        let entry = ChatEntry::ToolResult {
            id: id.to_owned(),
            lines,
        };
        match marker {
            Some(pos) => {
                let mut at = pos + 1;
                while matches!(self.entries.get(at), Some(ChatEntry::ToolResult { id: rid, .. }) if rid == id)
                {
                    at += 1;
                }
                self.entries.insert(at, entry);
            }
            None => self.entries.push(entry),
        }
    }

    /// Show the agent's plan. Plans arrive as full snapshots, so the
    /// previous block is replaced in place rather than appended.
    pub fn set_plan(&mut self, rows: Vec<PlanRow>) {
        for entry in self.entries.iter_mut().rev() {
            if let ChatEntry::Plan(existing) = entry {
                *existing = rows;
                return;
            }
        }
        self.entries.push(ChatEntry::Blank);
        self.entries.push(ChatEntry::Plan(rows));
    }

    /// Settle every still-running marker as failed. Called when a turn is
    /// cancelled — the updates that would settle them are never coming, and
    /// a marker left `Running` spins forever.
    pub fn fail_running_tools(&mut self) {
        for entry in self.entries.iter_mut() {
            if let ChatEntry::ToolMarker { status, .. } = entry
                && *status == ToolStatus::Running
            {
                *status = ToolStatus::Failure;
            }
        }
    }

    /// Flatten all entries into display lines for the chat widget.
    ///
    /// `frame` drives the animation for running tool markers (pass the
    /// current frame counter from the event loop).
    pub fn lines(&self, frame: u64) -> Vec<Line<'static>> {
        let mut out = Vec::new();
        for entry in &self.entries {
            match entry {
                ChatEntry::Text(lines) => out.extend(lines.iter().cloned()),
                ChatEntry::ToolMarker { label, status, .. } => {
                    let marker = match status {
                        ToolStatus::Running => {
                            Span::styled(spinner(frame), Style::new().add_modifier(Modifier::DIM))
                        }
                        ToolStatus::Success => Span::styled("⏺ ", Style::new().fg(GREEN)),
                        ToolStatus::Failure => Span::styled("⏺ ", Style::new().fg(RED)),
                    };
                    out.push(Line::from(vec![
                        marker,
                        Span::styled(
                            label.clone(),
                            Style::new().add_modifier(Modifier::BOLD | Modifier::DIM),
                        ),
                    ]));
                }
                ChatEntry::ToolResult { lines, .. } => out.extend(lines.iter().cloned()),
                ChatEntry::Thinking(lines) => out.extend(lines.iter().cloned()),
                ChatEntry::Plan(rows) => {
                    for row in rows {
                        let (mark, mark_style, text_style) = match row.status {
                            PlanStatus::Done => ("☒ ", Style::new().fg(GREEN), S_DIM),
                            PlanStatus::Active => (
                                "☐ ",
                                Style::new().fg(Color::Rgb(215, 119, 87)),
                                Style::new(),
                            ),
                            PlanStatus::Pending => ("☐ ", S_DIM, S_DIM),
                        };
                        out.push(Line::from(vec![
                            Span::raw("  "),
                            Span::styled(mark, mark_style),
                            Span::styled(row.text.clone(), text_style),
                        ]));
                    }
                }
                ChatEntry::Blank => out.push(Line::raw("")),
            }
        }
        out
    }
}

/// Braille spinner frame, trailing space included so it drops in where a
/// settled `⏺ ` marker would go.
fn spinner(frame: u64) -> String {
    const BRAILLE: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
    format!("{} ", BRAILLE[(frame as usize / 2) % BRAILLE.len()])
}

// ── Style mapping ────────────────────────────────────────────────
//
// We bypass termimad's `CompoundStyle` / crossterm `ContentStyle` entirely
// because termimad 0.34 re-exports crossterm 0.29 while our workspace uses
// crossterm 0.28 (required by ratatui 0.29).  Since WE define the MadSkin,
// we know exactly what colours map to which `CompositeKind`.

/// Base style for a line kind.  Must mirror the SKIN definition in render.rs.
fn kind_base_style(kind: CompositeKind) -> Style {
    match kind {
        CompositeKind::Header(1) => Style::new().fg(Color::Cyan).add_modifier(Modifier::BOLD),
        CompositeKind::Header(2) => Style::new().fg(Color::Magenta).add_modifier(Modifier::BOLD),
        CompositeKind::Header(3) => Style::new().fg(Color::White).add_modifier(Modifier::BOLD),
        CompositeKind::Header(_) => Style::new().fg(Color::White).add_modifier(Modifier::BOLD),
        _ => Style::default(),
    }
}

/// Extra modifiers from compound-level markdown attributes.
fn compound_modifiers(compound: &termimad::minimad::Compound<'_>) -> Modifier {
    let mut m = Modifier::empty();
    if compound.bold {
        m |= Modifier::BOLD;
    }
    if compound.italic {
        m |= Modifier::ITALIC;
    }
    if compound.strikeout {
        m |= Modifier::CROSSED_OUT;
    }
    m
}

/// Left margin (in spaces) for a line kind.  Mirrors the SKIN definition.
fn kind_left_margin(kind: CompositeKind) -> usize {
    match kind {
        CompositeKind::Code => 4,
        _ => 2,
    }
}

// ── Markdown → ratatui Lines ─────────────────────────────────────

/// Render a markdown string through the given `MadSkin` (for wrapping and
/// structure) and convert to ratatui `Line` values.
pub fn markdown_to_lines(skin: &MadSkin, md: &str, width: usize) -> Vec<Line<'static>> {
    let fmt = FmtText::from(skin, md, Some(width));
    let mut out = Vec::with_capacity(fmt.lines.len());
    for fl in &fmt.lines {
        match fl {
            FmtLine::Normal(composite) => {
                let base = kind_base_style(composite.kind);
                let margin = kind_left_margin(composite.kind);
                let mut spans = Vec::new();
                if margin > 0 {
                    spans.push(Span::raw(" ".repeat(margin)));
                }
                for compound in &composite.compounds {
                    let extra = compound_modifiers(compound);
                    let style = if extra.is_empty() {
                        base
                    } else {
                        base.add_modifier(extra)
                    };
                    spans.push(Span::styled(compound.src.to_string(), style));
                }
                out.push(Line::from(spans));
            }
            FmtLine::TableRow(row) => {
                let mut spans = Vec::new();
                spans.push(Span::raw("  │"));
                for cell in &row.cells {
                    for compound in &cell.compounds {
                        spans.push(Span::raw(compound.src.to_string()));
                    }
                    spans.push(Span::raw("│"));
                }
                out.push(Line::from(spans));
            }
            FmtLine::TableRule(rule) => {
                let total: usize = rule.widths.iter().sum::<usize>() + rule.widths.len() + 1;
                out.push(Line::raw(format!("  {}", "─".repeat(total))));
            }
            FmtLine::HorizontalRule => {
                out.push(Line::raw(format!(
                    "  {}",
                    "─".repeat(width.saturating_sub(2))
                )));
            }
        }
    }
    out
}
