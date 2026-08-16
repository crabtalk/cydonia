//! Chat buffer — semantic cells for streaming chat output.
//!
//! Entries store *source* (markdown text, tool output), not rendered
//! lines. Rendering happens at display time as a pure function of
//! (cell, width) with a single-entry cache per markdown cell, so the
//! transcript rewraps on resize and streaming only re-renders the cell
//! it touches. Output style matches Claude Code: `⏺` markers for text
//! and tool calls, `⎿` for tool results, 2-space continuation indent.

use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span},
};
use std::sync::LazyLock;
use termimad::{CompositeKind, FmtLine, FmtText, MadSkin};

// ── Brand colours (same palette as the old renderer) ─────────────

pub const GREEN: Color = Color::Indexed(71);
pub const RED: Color = Color::Indexed(204);
pub const SUBTLE: Color = Color::Indexed(240);

pub const S_DIM: Style = Style::new().add_modifier(Modifier::DIM);
pub const S_SUBTLE: Style = Style::new().fg(SUBTLE);

/// Text continuation indent (aligns with text after `⏺ `).
const PAD: &str = "  ";
const TOOL_PAD: &str = "  ";
const TOOL_OUTPUT_MAX_SUCCESS: usize = 5;
const TOOL_OUTPUT_MAX_FAILURE: usize = 10;

/// Skin with 2-space left margin (for text continuation after `⏺ `).
pub static SKIN: LazyLock<MadSkin> = LazyLock::new(|| {
    use termimad::crossterm::style::{Attribute, Color};
    let mut skin = MadSkin::default_dark();
    skin.paragraph.left_margin = 2;
    skin.headers[0]
        .compound_style
        .set_fgbg(Color::Cyan, Color::Reset);
    skin.headers[0].compound_style.add_attr(Attribute::Bold);
    skin.headers[0].left_margin = 2;
    skin.headers[1]
        .compound_style
        .set_fgbg(Color::Magenta, Color::Reset);
    skin.headers[1].compound_style.add_attr(Attribute::Bold);
    skin.headers[1].left_margin = 2;
    skin.headers[2]
        .compound_style
        .set_fgbg(Color::White, Color::Reset);
    skin.headers[2].compound_style.add_attr(Attribute::Bold);
    skin.headers[2].left_margin = 2;
    skin.code_block.left_margin = 4;
    skin
});

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

/// Agent prose, accumulated as markdown source while streaming.
///
/// Only the source up to the last newline is rendered (a partial
/// markdown line can change meaning when the rest arrives); the
/// remainder shows as a raw tail line until it completes. On top of
/// that, `revealed` gates how much of the committed source displays —
/// [`ChatBuffer::commit_tick`] advances it one line per frame for a
/// typing feel, jumping when a backlog builds (codex's two-gear
/// chunking, depth-only hysteresis).
#[derive(Debug)]
pub struct MarkdownCell {
    source: String,
    done: bool,
    /// Display cap into the committed source, at a line boundary.
    revealed: usize,
    /// `(width, shown_len)` → rendered lines. Width rarely changes
    /// and the shown prefix only grows, so one entry is enough.
    cache: Option<(usize, usize, Vec<Line<'static>>)>,
}

impl MarkdownCell {
    fn new(chunk: &str) -> Self {
        Self {
            source: chunk.to_owned(),
            done: false,
            revealed: 0,
            cache: None,
        }
    }

    fn committed_len(&self) -> usize {
        if self.done {
            self.source.len()
        } else {
            self.source.rfind('\n').map_or(0, |ix| ix + 1)
        }
    }

    fn lines(&mut self, width: usize) -> Vec<Line<'static>> {
        let committed = self.committed_len();
        let shown = committed.min(self.revealed);
        let stale = self
            .cache
            .as_ref()
            .is_none_or(|(w, s, _)| *w != width || *s != shown);
        if stale {
            let rendered = render_block(&self.source[..shown], width);
            self.cache = Some((width, shown, rendered));
        }
        let (_, _, rendered) = self.cache.as_ref().expect("filled above");
        let mut out = rendered.clone();

        // The raw tail only shows once the reveal has caught up —
        // otherwise it would display text ahead of the reveal point.
        if self.revealed >= committed && !self.done {
            let tail = self.source[committed..].trim_end_matches('\n');
            if !tail.is_empty() {
                let prefix = if out.is_empty() {
                    Span::styled("⏺ ", S_DIM)
                } else {
                    Span::raw(PAD.to_string())
                };
                out.push(Line::from(vec![prefix, Span::raw(tail.to_owned())]));
            }
        }
        out
    }
}

/// Render a completed markdown block: termimad for structure and
/// wrapping, `⏺` marker replacing the margin on the first content line.
fn render_block(source: &str, width: usize) -> Vec<Line<'static>> {
    let mut lines = markdown_to_lines(&SKIN, source, width);
    let blank = |line: &Line| {
        line.spans
            .iter()
            .all(|s| s.content.chars().all(char::is_whitespace))
    };
    while lines.first().is_some_and(&blank) {
        lines.remove(0);
    }
    while lines.last().is_some_and(&blank) {
        lines.pop();
    }
    if let Some(first) = lines.first_mut() {
        let spans: Vec<Span> = std::mem::take(&mut first.spans)
            .into_iter()
            .skip_while(|s| s.content.chars().all(|c| c == ' '))
            .collect();
        let mut new_spans = vec![Span::styled("⏺ ", S_DIM)];
        new_spans.extend(spans);
        *first = Line::from(new_spans);
    }
    lines
}

/// One logical chunk in the chat output.
#[derive(Debug)]
pub enum ChatEntry {
    /// Pre-styled lines (header box, user echo, notices) — width-independent.
    Text(Vec<Line<'static>>),
    /// Agent prose, rendered from markdown source at display time.
    Markdown(MarkdownCell),
    /// Thinking / reasoning text. While streaming it renders as one
    /// stable peek line (latest thought + elapsed) so the transcript
    /// doesn't shift; once done, the full text shows dimmed.
    Thinking {
        text: String,
        done: bool,
        started: std::time::Instant,
    },
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
        output: String,
        failed: bool,
    },
    /// The agent's plan checklist.
    Plan(Vec<PlanRow>),
    /// Blank separator line.
    Blank,
}

/// Append-only buffer of chat entries; also the sink for streaming
/// agent output (chunks append to open cells, blocks seal on
/// transitions).
#[derive(Debug, Default)]
pub struct ChatBuffer {
    pub entries: Vec<ChatEntry>,
    /// True while waiting for the first content of a response (drives
    /// the waiting spinner).
    pub waiting: bool,
    /// Reveal gear: `true` drains the whole backlog per tick instead
    /// of one line (entered at ≥8 pending lines, exited at ≤2).
    catch_up: bool,
}

/// Backlog depth that flips the reveal into catch-up.
const REVEAL_ENTER_LINES: usize = 8;
/// Backlog depth that drops it back to one line per tick.
const REVEAL_EXIT_LINES: usize = 2;

impl ChatBuffer {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, entry: ChatEntry) {
        self.entries.push(entry);
    }

    /// Signal that a new response is coming; closes open cells so the
    /// next text starts a fresh `⏺` block.
    pub fn start_waiting(&mut self) {
        self.waiting = true;
        self.seal();
    }

    /// The turn settled: close open cells, stop the spinner.
    pub fn finish(&mut self) {
        self.waiting = false;
        self.seal();
    }

    fn seal(&mut self) {
        for entry in &mut self.entries {
            match entry {
                ChatEntry::Markdown(cell) => {
                    cell.done = true;
                    cell.revealed = cell.source.len();
                }
                ChatEntry::Thinking { done, .. } => *done = true,
                _ => {}
            }
        }
    }

    /// Advance the reveal of the open markdown cell by one line — or by
    /// the whole backlog while in catch-up. Called once per frame.
    pub fn commit_tick(&mut self) {
        let catch_up = &mut self.catch_up;
        let Some(ChatEntry::Markdown(cell)) = self.entries.last_mut() else {
            return;
        };
        let committed = cell.committed_len();
        if cell.done || cell.revealed >= committed {
            return;
        }
        let pending = cell.source[cell.revealed..committed].matches('\n').count();
        if !*catch_up && pending >= REVEAL_ENTER_LINES {
            *catch_up = true;
        } else if *catch_up && pending <= REVEAL_EXIT_LINES {
            *catch_up = false;
        }
        if *catch_up {
            cell.revealed = committed;
        } else if let Some(ix) = cell.source[cell.revealed..committed].find('\n') {
            cell.revealed += ix + 1;
        }
    }

    /// Whether committed content is still waiting to be revealed
    /// (drives the fast frame re-arm).
    pub fn revealing(&self) -> bool {
        matches!(
            self.entries.last(),
            Some(ChatEntry::Markdown(cell)) if !cell.done && cell.revealed < cell.committed_len()
        )
    }

    fn separate(&mut self) {
        if !matches!(self.entries.last(), None | Some(ChatEntry::Blank)) {
            self.entries.push(ChatEntry::Blank);
        }
    }

    /// Append streamed agent text to the open markdown cell (or start
    /// a new block).
    pub fn push_text(&mut self, chunk: &str) {
        if chunk.is_empty() {
            return;
        }
        self.waiting = false;
        if let Some(ChatEntry::Thinking { done, .. }) = self.entries.last_mut() {
            *done = true;
        }
        if let Some(ChatEntry::Markdown(cell)) = self.entries.last_mut()
            && !cell.done
        {
            cell.source.push_str(chunk);
            return;
        }
        self.separate();
        self.entries
            .push(ChatEntry::Markdown(MarkdownCell::new(chunk)));
    }

    /// Append streamed thinking text.
    pub fn push_thinking(&mut self, chunk: &str) {
        if chunk.is_empty() {
            return;
        }
        self.waiting = false;
        if let Some(ChatEntry::Thinking {
            text, done: false, ..
        }) = self.entries.last_mut()
        {
            text.push_str(chunk);
            return;
        }
        self.separate();
        self.entries.push(ChatEntry::Thinking {
            text: chunk.to_owned(),
            done: false,
            started: std::time::Instant::now(),
        });
    }

    /// Start a tool call marker. `label` is the agent-provided title.
    pub fn push_tool_call(&mut self, id: &str, label: String) {
        self.waiting = false;
        self.seal();
        self.separate();
        self.entries.push(ChatEntry::ToolMarker {
            id: id.to_owned(),
            label,
            status: ToolStatus::Running,
        });
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

    /// Attach result output under the marker with the given call id,
    /// after any results already attached to it. Falls back to
    /// appending at the end if the marker is gone (e.g. after /clear).
    pub fn push_tool_result(&mut self, id: &str, output: &str, failed: bool) {
        let entry = ChatEntry::ToolResult {
            id: id.to_owned(),
            output: output.to_owned(),
            failed,
        };
        let marker = self
            .entries
            .iter()
            .position(|e| matches!(e, ChatEntry::ToolMarker { id: mid, .. } if mid == id));
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

    /// Whether something on screen animates (drives frame re-arm).
    pub fn animating(&self) -> bool {
        self.waiting
            || self.entries.iter().any(|e| {
                matches!(
                    e,
                    ChatEntry::ToolMarker {
                        status: ToolStatus::Running,
                        ..
                    } | ChatEntry::Thinking { done: false, .. }
                )
            })
    }

    /// Flatten all entries into display lines at `width`.
    ///
    /// `frame` drives the animation for running tool markers (pass the
    /// current frame counter from the event loop).
    pub fn lines(&mut self, frame: u64, width: usize) -> Vec<Line<'static>> {
        let mut out = Vec::new();
        for entry in &mut self.entries {
            match entry {
                ChatEntry::Text(lines) => out.extend(lines.iter().cloned()),
                ChatEntry::Markdown(cell) => out.extend(cell.lines(width)),
                ChatEntry::Thinking {
                    text,
                    done,
                    started,
                } => {
                    let style = Style::new().add_modifier(Modifier::DIM | Modifier::ITALIC);
                    if *done {
                        out.extend(
                            text.lines()
                                .map(|l| Line::from(Span::styled(l.to_owned(), style))),
                        );
                    } else {
                        // One stable peek line: latest thought + elapsed.
                        let latest = text
                            .lines()
                            .rev()
                            .find(|l| !l.trim().is_empty())
                            .unwrap_or("thinking");
                        let secs = started.elapsed().as_secs();
                        out.push(Line::from(vec![
                            Span::styled("✳ ", S_DIM),
                            Span::styled(truncate_chars(latest, width.saturating_sub(12)), style),
                            Span::styled(format!(" · {secs}s"), S_DIM),
                        ]));
                    }
                }
                ChatEntry::ToolMarker { label, status, .. } => {
                    let marker = match status {
                        ToolStatus::Running => Span::styled(spinner(frame), S_DIM),
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
                ChatEntry::ToolResult { output, failed, .. } => {
                    out.extend(tool_result_lines(output, *failed, width));
                }
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

// ── Tool result rendering ────────────────────────────────────────

/// `⎿`-prefixed result lines with a line cap and a width-aware,
/// char-boundary-safe budget per line (a single pathological long line
/// can't blow up the layout, and multi-byte text can't panic a slice).
fn tool_result_lines(output: &str, failed: bool, width: usize) -> Vec<Line<'static>> {
    let max_lines = if failed {
        TOOL_OUTPUT_MAX_FAILURE
    } else {
        TOOL_OUTPUT_MAX_SUCCESS
    };
    let budget = width.saturating_sub(TOOL_PAD.len() + 2).max(20);

    let mut shown = Vec::new();
    let mut total = 0usize;
    for line in output.lines().filter(|l| !l.is_empty()) {
        total += 1;
        if shown.len() < max_lines {
            shown.push(truncate_chars(line, budget));
        }
    }

    let mut lines = Vec::new();
    if shown.is_empty() {
        lines.push(Line::from(vec![
            Span::raw(TOOL_PAD.to_string()),
            Span::styled("⎿ ", S_SUBTLE),
            Span::styled("(no output)", S_DIM),
        ]));
    } else {
        for (i, line) in shown.iter().enumerate() {
            if i == 0 {
                lines.push(Line::from(vec![
                    Span::raw(TOOL_PAD.to_string()),
                    Span::styled("⎿ ", S_SUBTLE),
                    Span::styled(line.clone(), S_DIM),
                ]));
            } else {
                lines.push(Line::from(vec![
                    Span::raw(format!("{TOOL_PAD}  ")),
                    Span::styled(line.clone(), S_DIM),
                ]));
            }
        }
        if total > shown.len() {
            lines.push(Line::from(vec![
                Span::raw(format!("{TOOL_PAD}  ")),
                Span::styled(format!("… +{} lines", total - shown.len()), S_DIM),
            ]));
        }
    }
    lines.push(Line::raw(""));
    lines
}

fn truncate_chars(line: &str, max: usize) -> String {
    if line.chars().count() <= max {
        return line.to_owned();
    }
    let cut: String = line.chars().take(max.saturating_sub(1)).collect();
    format!("{cut}…")
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

/// Base style for a line kind.  Must mirror [`SKIN`].
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

/// Left margin (in spaces) for a line kind.  Mirrors [`SKIN`].
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
