//! Reusable TUI components for cydonia screens.

use anyhow::Result;
use crossterm::{
    event::{self, DisableBracketedPaste, EnableBracketedPaste, Event, KeyCode},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{
    Terminal,
    backend::CrosstermBackend,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
};
use std::io::Stdout;

/// Run a full-screen TUI application, returning the final state on exit.
///
/// Handles terminal setup, the event loop, and teardown (including panic
/// recovery). `init` creates the initial state, and each iteration calls
/// `draw` then `handle_key`. The loop exits when `handle_key` returns
/// `Some(result)`.
pub fn run_app_with_state<S>(
    init: impl FnOnce() -> Result<S>,
    draw: impl Fn(&mut ratatui::Frame, &S),
    handle_key: impl Fn(event::KeyEvent, &mut S) -> Result<Option<Result<()>>>,
) -> Result<S> {
    let mut terminal = setup()?;
    let mut state = init()?;
    let result = event_loop(&mut terminal, &mut state, &draw, &handle_key);
    teardown(&mut terminal)?;
    result?;
    Ok(state)
}

/// Prepare terminal for TUI. Returns the terminal handle.
/// Call `teardown` when done.
pub fn setup() -> Result<Terminal<CrosstermBackend<Stdout>>> {
    enable_raw_mode()?;
    let mut stdout = std::io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableBracketedPaste)?;

    let original_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = disable_raw_mode();
        let _ = execute!(
            std::io::stdout(),
            DisableBracketedPaste,
            LeaveAlternateScreen
        );
        original_hook(info);
    }));

    Ok(Terminal::new(CrosstermBackend::new(stdout))?)
}

/// Restore terminal after TUI.
pub fn teardown(terminal: &mut Terminal<CrosstermBackend<Stdout>>) -> Result<()> {
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        DisableBracketedPaste,
        LeaveAlternateScreen
    )?;
    terminal.show_cursor()?;
    Ok(())
}

fn event_loop<S>(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    state: &mut S,
    draw: &impl Fn(&mut ratatui::Frame, &S),
    handle_key: &impl Fn(event::KeyEvent, &mut S) -> Result<Option<Result<()>>>,
) -> Result<()> {
    loop {
        terminal.draw(|frame| draw(frame, state))?;
        if event::poll(std::time::Duration::from_millis(250))?
            && let Event::Key(key) = event::read()?
            && let Some(result) = handle_key(key, state)?
        {
            return result;
        }
    }
}

// ── Text editing helpers ────────────────────────────────────────────

/// Convert a char index to a byte offset within a string.
pub fn char_to_byte(s: &str, char_idx: usize) -> usize {
    s.char_indices()
        .nth(char_idx)
        .map(|(b, _)| b)
        .unwrap_or(s.len())
}

/// Handle standard text-input key events on a buffer + cursor.
pub fn handle_text_input(code: KeyCode, buf: &mut String, cursor: &mut usize) {
    match code {
        KeyCode::Backspace if *cursor > 0 => {
            let start = char_to_byte(buf, *cursor - 1);
            let end = char_to_byte(buf, *cursor);
            buf.drain(start..end);
            *cursor -= 1;
        }
        KeyCode::Delete => {
            let char_count = buf.chars().count();
            if *cursor < char_count {
                let start = char_to_byte(buf, *cursor);
                let end = char_to_byte(buf, *cursor + 1);
                buf.drain(start..end);
            }
        }
        KeyCode::Left => {
            *cursor = cursor.saturating_sub(1);
        }
        KeyCode::Right => {
            let char_count = buf.chars().count();
            if *cursor < char_count {
                *cursor += 1;
            }
        }
        KeyCode::Home => {
            *cursor = 0;
        }
        KeyCode::End => {
            *cursor = buf.chars().count();
        }
        KeyCode::Char(c) => {
            let byte_pos = char_to_byte(buf, *cursor);
            buf.insert(byte_pos, c);
            *cursor += 1;
        }
        _ => {}
    }
}

// ── Style helpers ───────────────────────────────────────────────────

/// Border style for a focused panel (brand orange).
pub fn border_focused() -> Style {
    Style::default().fg(ACCENT)
}

// ── Modal screens ────────────────────────────────────────────────
//
// The picker, the agent and MCP screens, the permission prompt and the
// installer are all the same thing: a centred box holding a scrolling
// list and a hint. These are the parts they share.

/// The accent used for the selected row and other highlights.
pub const ACCENT: Color = Color::Rgb(215, 119, 87);

/// A centred box of at most `width` × `height`, clamped to `area`.
pub fn centered(area: Rect, width: u16, height: u16) -> Rect {
    let width = width.min(area.width);
    let height = height.min(area.height);
    Rect::new(
        area.width.saturating_sub(width) / 2,
        area.height.saturating_sub(height) / 2,
        width,
        height,
    )
}

/// Truncate to `max` characters, the ellipsis counted within it.
pub fn truncate(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_owned();
    }
    text.chars().take(max.saturating_sub(3)).collect::<String>() + "..."
}

/// The first visible index that keeps `selected` inside a window of
/// `visible` rows: still until the selection would leave the bottom,
/// then trailing it one row at a time.
pub fn window(selected: usize, visible: usize) -> usize {
    selected.saturating_sub(visible.saturating_sub(1))
}

/// One list row — `> label — detail` — with the detail dimmed, and
/// dropped entirely when the row leaves no room for it.
pub fn row(label: &str, detail: &str, selected: bool, inner: usize) -> Line<'static> {
    let (marker, style) = if selected {
        ("> ", Style::new().fg(ACCENT).add_modifier(Modifier::BOLD))
    } else {
        ("  ", Style::new().fg(Color::Gray))
    };
    let mut spans = vec![
        Span::styled(marker, style),
        Span::styled(label.to_owned(), style),
    ];
    if !detail.is_empty() {
        let room = inner.saturating_sub(2 + label.chars().count() + 3);
        if room >= 10 {
            spans.push(Span::styled(
                format!(" — {}", truncate(detail, room)),
                Style::new().add_modifier(Modifier::DIM),
            ));
        }
    }
    Line::from(spans)
}

/// Draw a bordered modal over whatever is beneath it: `lines`, a blank,
/// then the dim `hint`.
pub fn modal(
    frame: &mut ratatui::Frame,
    rect: Rect,
    title: &str,
    mut lines: Vec<Line<'static>>,
    hint: &str,
) {
    lines.push(Line::raw(""));
    lines.push(Line::from(Span::styled(
        hint.to_owned(),
        Style::new().add_modifier(Modifier::DIM),
    )));
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(border_focused())
        .title(title.to_owned());
    frame.render_widget(Clear, rect);
    frame.render_widget(Paragraph::new(lines).block(block), rect);
}
