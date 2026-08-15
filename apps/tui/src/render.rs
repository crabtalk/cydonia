//! Streaming markdown renderer producing ratatui `Line` values.
//!
//! Output style matches Claude Code: `⏺` markers for text and tool calls,
//! `⎿` for tool results, 2-space continuation indent.
//!
//! All output goes into a [`ChatBuffer`] — nothing is written to stdout.

use crate::chat::{ChatBuffer, ChatEntry, S_DIM, S_SUBTLE, SUBTLE, ToolStatus, markdown_to_lines};
use ratatui::{
    style::{Modifier, Style},
    text::{Line, Span},
};
use std::sync::LazyLock;
use termimad::MadSkin;

/// Text continuation indent (aligns with text after `⏺ `).
const PAD: &str = "  ";
const TOOL_OUTPUT_MAX_SUCCESS: usize = 5;
const TOOL_OUTPUT_MAX_FAILURE: usize = 10;
const TOOL_PAD: &str = "  ";

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

#[derive(Default)]
enum RenderState {
    #[default]
    Normal,
    CodeBlock {
        lang: String,
        code: String,
    },
    Table(String),
    Thinking,
}

pub struct MarkdownRenderer {
    line_buf: String,
    state: RenderState,
    pub buffer: ChatBuffer,
    width: usize,
    /// Whether we've emitted the first `⏺` marker for this response.
    started: bool,
    /// Whether the next text line is the very first in the response.
    first_line: bool,
    /// Whether the last thing emitted was a blank line (avoid double blanks).
    last_was_blank: bool,
    after_tool: bool,
    /// True while waiting for content (drives spinner in the UI).
    pub waiting: bool,
    /// Accumulator for thinking text (emitted as one entry when switching back).
    thinking_buf: String,
}

impl Default for MarkdownRenderer {
    fn default() -> Self {
        Self::new()
    }
}

impl MarkdownRenderer {
    pub fn new() -> Self {
        Self {
            line_buf: String::new(),
            state: RenderState::Normal,
            buffer: ChatBuffer::new(),
            width: 80,
            started: false,
            first_line: false,
            last_was_blank: false,
            after_tool: false,
            waiting: false,
            thinking_buf: String::new(),
        }
    }

    pub fn set_width(&mut self, width: usize) {
        self.width = width;
    }

    /// Signal that we're waiting for a new response.
    ///
    /// Resets streaming state so the first text of the new response gets a
    /// fresh `⏺` marker.  Without this, `started`/`first_line` from the
    /// previous response would bleed through and suppress the marker.
    pub fn start_waiting(&mut self) {
        self.waiting = true;
        self.started = false;
        self.first_line = false;
        self.after_tool = false;
    }

    /// The partially-streamed current line (displayed at the bottom of chat).
    pub fn current_line(&self) -> Option<Line<'static>> {
        if self.line_buf.is_empty() {
            return None;
        }
        if self.first_line || !self.started {
            // First line gets the ⏺ marker prefix.
            Some(Line::from(vec![
                Span::styled("⏺ ", Style::new().add_modifier(Modifier::DIM)),
                Span::raw(self.line_buf.clone()),
            ]))
        } else {
            Some(Line::from(vec![
                Span::raw(PAD.to_string()),
                Span::raw(self.line_buf.clone()),
            ]))
        }
    }

    pub fn push_text(&mut self, chunk: &str) {
        if chunk.is_empty() {
            return;
        }
        self.waiting = false;

        if matches!(self.state, RenderState::Thinking) {
            self.flush_thinking();
        }

        self.ensure_started();

        if self.after_tool {
            self.after_tool = false;
            self.first_line = true;
        }

        for ch in chunk.chars() {
            if ch == '\n' {
                let line = std::mem::take(&mut self.line_buf);
                let non_empty = !line.is_empty();
                self.render_line(&line);
                // Only consume first_line on actual content — blank lines
                // (common at response start) must not eat the ⏺ marker.
                if non_empty {
                    self.first_line = false;
                }
            } else {
                self.line_buf.push(ch);
            }
        }
    }

    pub fn push_thinking(&mut self, chunk: &str) {
        if chunk.is_empty() {
            return;
        }
        self.waiting = false;

        if !matches!(self.state, RenderState::Thinking) {
            self.state = RenderState::Thinking;
        }
        self.thinking_buf.push_str(chunk);
    }

    /// Start a tool call marker. `label` is the agent-provided title.
    pub fn push_tool_call(&mut self, id: &str, label: String) {
        self.flush_thinking();
        self.finalize_line_buf();
        self.waiting = false;
        self.buffer.push(ChatEntry::Blank);
        self.buffer.push(ChatEntry::ToolMarker {
            id: id.to_owned(),
            label,
            status: ToolStatus::Running,
        });
        // Text after a tool call starts a fresh ⏺ block.
        self.after_tool = true;
    }

    pub fn set_tool_status(&mut self, id: &str, status: ToolStatus) {
        self.buffer.set_tool_status(id, status);
    }

    pub fn set_tool_label(&mut self, id: &str, label: String) {
        self.buffer.set_tool_label(id, label);
    }

    /// Attach result text under the tool call's marker.
    pub fn push_tool_result(&mut self, id: &str, output: &str, failed: bool) {
        let max = if failed {
            TOOL_OUTPUT_MAX_FAILURE
        } else {
            TOOL_OUTPUT_MAX_SUCCESS
        };
        let (text_lines, total) = truncate_lines(output, max);
        let max_width = self.width.saturating_sub(TOOL_PAD.len() + 2);

        let mut lines = Vec::new();

        if text_lines.is_empty() {
            lines.push(Line::from(vec![
                Span::raw(TOOL_PAD.to_string()),
                Span::styled("⎿ ", S_SUBTLE),
                Span::styled("(no output)", S_DIM),
            ]));
        } else {
            for (i, line) in text_lines.iter().enumerate() {
                let truncated = if line.len() > max_width {
                    format!("{}...", &line[..max_width.saturating_sub(3)])
                } else {
                    line.clone()
                };
                if i == 0 {
                    lines.push(Line::from(vec![
                        Span::raw(TOOL_PAD.to_string()),
                        Span::styled("⎿ ", S_SUBTLE),
                        Span::styled(truncated, S_DIM),
                    ]));
                } else {
                    lines.push(Line::from(vec![
                        Span::raw(format!("{TOOL_PAD}  ")),
                        Span::styled(truncated, S_DIM),
                    ]));
                }
            }
            let shown = text_lines.len();
            if total > shown {
                lines.push(Line::from(vec![
                    Span::raw(format!("{TOOL_PAD}  ")),
                    Span::styled(format!("… +{} lines", total - shown), S_DIM),
                ]));
            }
        }
        lines.push(Line::raw(""));

        self.buffer.insert_tool_result(id, lines);
    }

    pub fn finish(&mut self) {
        self.flush_thinking();
        self.finalize_line_buf();
        self.waiting = false;
    }

    // ── Internal ─────────────────────────────────────────────────

    fn ensure_started(&mut self) {
        if !self.started {
            self.waiting = false;
            self.started = true;
            self.first_line = true;
        }
    }

    /// Finalize any partial line_buf content into the buffer.
    fn finalize_line_buf(&mut self) {
        if self.line_buf.is_empty() {
            return;
        }
        let line = std::mem::take(&mut self.line_buf);
        match &self.state {
            RenderState::CodeBlock { .. } => {
                self.flush_code_block_raw(&line);
            }
            _ => {
                // Use ⏺ marker on the first line, PAD otherwise.
                let prefix = if self.first_line {
                    Span::styled("⏺ ", Style::new().add_modifier(Modifier::DIM))
                } else {
                    Span::raw(PAD.to_string())
                };
                self.buffer.push(ChatEntry::Text(vec![Line::from(vec![
                    prefix,
                    Span::raw(line),
                ])]));
            }
        }
        self.first_line = false;
    }

    fn render_line(&mut self, line: &str) {
        match &mut self.state {
            RenderState::CodeBlock { lang: _, code } => {
                if line.starts_with("```") {
                    let lang = if let RenderState::CodeBlock { lang, .. } = &self.state {
                        lang.clone()
                    } else {
                        String::new()
                    };
                    let code = if let RenderState::CodeBlock { code, .. } = &self.state {
                        code.clone()
                    } else {
                        String::new()
                    };
                    self.emit_code_block(&lang, &code);
                    self.state = RenderState::Normal;
                } else {
                    code.push_str(line);
                    code.push('\n');
                }
            }
            RenderState::Table(buf) => {
                if line.starts_with('|') || line.starts_with("|-") {
                    buf.push_str(line);
                    buf.push('\n');
                } else {
                    self.flush_table();
                    self.render_line(line);
                }
            }
            RenderState::Normal | RenderState::Thinking => {
                if let Some(rest) = line.strip_prefix("```") {
                    let lang = rest.trim().to_string();
                    self.emit_code_border_top(&lang);
                    self.state = RenderState::CodeBlock {
                        lang,
                        code: String::new(),
                    };
                } else if line.starts_with('|') {
                    let mut buf = String::new();
                    buf.push_str(line);
                    buf.push('\n');
                    self.state = RenderState::Table(buf);
                } else {
                    self.render_md_line(line);
                }
            }
        }
    }

    fn flush_table(&mut self) {
        if let RenderState::Table(buf) = &self.state {
            let lines = markdown_to_lines(&SKIN, buf, self.width.saturating_sub(PAD.len()));
            self.buffer.push(ChatEntry::Text(lines));
        }
        self.state = RenderState::Normal;
    }

    fn render_md_line(&mut self, line: &str) {
        if line.is_empty() {
            self.buffer.push(ChatEntry::Blank);
            self.last_was_blank = true;
            return;
        }
        self.last_was_blank = false;

        let mut lines = markdown_to_lines(&SKIN, line, self.width);

        // On the first line, prepend the ⏺ marker.
        if self.first_line && !lines.is_empty() {
            let first = lines.remove(0);
            // Strip the skin's left margin (we replace it with ⏺ marker).
            let spans: Vec<Span> = first
                .spans
                .into_iter()
                .skip_while(|s| s.content.chars().all(|c| c == ' '))
                .collect();
            let mut new_spans = vec![Span::styled("⏺ ", Style::new().add_modifier(Modifier::DIM))];
            new_spans.extend(spans);
            lines.insert(0, Line::from(new_spans));
        }

        self.buffer.push(ChatEntry::Text(lines));
    }

    fn emit_code_border_top(&mut self, lang: &str) {
        self.first_line = false;
        let label = if lang.is_empty() {
            "┌─".to_string()
        } else {
            format!("┌ {lang} ─")
        };
        self.buffer.push(ChatEntry::Text(vec![Line::from(vec![
            Span::raw(PAD.to_string()),
            Span::styled(label, S_SUBTLE),
        ])]));
    }

    fn emit_code_block(&mut self, _lang: &str, code: &str) {
        let pipe_style = Style::new().fg(SUBTLE);
        let mut lines = Vec::new();
        for line in code.lines() {
            lines.push(Line::from(vec![
                Span::raw(PAD.to_string()),
                Span::styled("│ ", pipe_style),
                Span::raw(line.to_string()),
            ]));
        }
        lines.push(Line::from(vec![
            Span::raw(PAD.to_string()),
            Span::styled("└─", S_SUBTLE),
        ]));
        self.buffer.push(ChatEntry::Text(lines));
    }

    fn flush_code_block_raw(&mut self, extra: &str) {
        if let RenderState::CodeBlock { code, .. } = &self.state {
            let full = format!("{code}{extra}");
            let pipe_style = Style::new().fg(SUBTLE);
            let mut lines = Vec::new();
            for line in full.lines() {
                lines.push(Line::from(vec![
                    Span::raw(PAD.to_string()),
                    Span::styled("│ ", pipe_style),
                    Span::raw(line.to_string()),
                ]));
            }
            lines.push(Line::from(vec![
                Span::raw(PAD.to_string()),
                Span::styled("└─", S_SUBTLE),
            ]));
            self.buffer.push(ChatEntry::Text(lines));
        }
        self.state = RenderState::Normal;
    }

    pub fn flush_thinking(&mut self) {
        if !self.thinking_buf.is_empty() {
            let text = std::mem::take(&mut self.thinking_buf);
            let thinking_style = Style::new().add_modifier(Modifier::DIM | Modifier::ITALIC);
            let lines: Vec<Line<'static>> = text
                .lines()
                .map(|l| Line::from(Span::styled(l.to_string(), thinking_style)))
                .collect();
            if !lines.is_empty() {
                self.buffer.push(ChatEntry::Thinking(lines));
            }
        }
        if matches!(self.state, RenderState::Thinking) {
            self.state = RenderState::Normal;
        }
    }
}

// ── Tool output helpers ──────────────────────────────────────────

fn truncate_lines(text: &str, max: usize) -> (Vec<String>, usize) {
    let all: Vec<&str> = text.lines().filter(|l| !l.is_empty()).collect();
    let total = all.len();
    let lines = all.into_iter().take(max).map(String::from).collect();
    (lines, total)
}
