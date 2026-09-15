//! A session's shell: the PTY lives as long as this entity, even while hidden.

use crate::model::typography;
use bezel::{
    gpui::{
        self, App, ClipboardItem, Context, FocusHandle, Focusable, KeyBinding, Render, Task,
        Window, div, prelude::*, px,
    },
    theme::{TextStyle, Theme, Typeset},
    ui::{icons, input, tooltip::Tooltip},
};
use futures::{SinkExt, StreamExt, channel::mpsc};
use portable_pty::{ChildKiller, CommandBuilder, MasterPty, PtySize, native_pty_system};
use std::{
    io::{Read, Write},
    path::Path,
    sync::mpsc as channel,
};
use terminal::{
    emulator::{Emulator, SelectionType},
    view::{self, GridGeometry, GridSnapshot, TerminalElement},
};

const CONTEXT: &str = "CydoniaTerminal";

gpui::actions!(
    cydonia_terminal,
    [IncreaseTextSize, DecreaseTextSize, ResetTextSize]
);

pub fn bindings() -> Vec<KeyBinding> {
    vec![
        KeyBinding::new("cmd-c", input::Copy, Some(CONTEXT)),
        KeyBinding::new("cmd-v", input::Paste, Some(CONTEXT)),
        KeyBinding::new("cmd-=", IncreaseTextSize, Some(CONTEXT)),
        KeyBinding::new("cmd-+", IncreaseTextSize, Some(CONTEXT)),
        KeyBinding::new("cmd-shift-=", IncreaseTextSize, Some(CONTEXT)),
        KeyBinding::new("cmd--", DecreaseTextSize, Some(CONTEXT)),
        KeyBinding::new("cmd-0", ResetTextSize, Some(CONTEXT)),
    ]
}

/// Option sends Meta using the base key, not its macOS alternate character.
pub fn keystroke_bytes(key: &gpui::Keystroke, app_cursor: bool) -> Option<Vec<u8>> {
    let meta_char = if key.modifiers.alt && !key.modifiers.control && key.key.chars().count() == 1 {
        Some(if key.modifiers.shift {
            key.key.to_uppercase()
        } else {
            key.key.clone()
        })
    } else {
        None
    };
    view::keystroke_bytes(
        &key.key,
        meta_char.as_deref().or(key.key_char.as_deref()),
        &key.modifiers,
        app_cursor,
    )
}

/// Dropping the panel's owner terminates the shell; a waiter reaps it off-thread.
struct Shell {
    master: Box<dyn MasterPty + Send>,
    input: channel::Sender<Vec<u8>>,
    killer: Box<dyn ChildKiller + Send + Sync>,
}

impl Drop for Shell {
    fn drop(&mut self) {
        let _ = self.killer.kill();
    }
}

impl Shell {
    fn open(cwd: &Path) -> anyhow::Result<(Self, mpsc::Receiver<Vec<u8>>)> {
        let shell = std::env::var_os("SHELL")
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "/bin/zsh".into());
        let mut command = CommandBuilder::new(shell);
        command.arg("-l");
        Self::open_command(cwd, command)
    }

    fn open_command(
        cwd: &Path,
        mut command: CommandBuilder,
    ) -> anyhow::Result<(Self, mpsc::Receiver<Vec<u8>>)> {
        let pair = native_pty_system().openpty(PtySize {
            rows: 24,
            cols: 80,
            pixel_width: 0,
            pixel_height: 0,
        })?;
        let mut reader = pair.master.try_clone_reader()?;
        let mut writer = pair.master.take_writer()?;
        command.cwd(cwd);
        command.env("TERM", "xterm-256color");
        command.env("COLORTERM", "truecolor");
        let mut child = pair.slave.spawn_command(command)?;
        let killer = child.clone_killer();
        drop(pair.slave);
        std::thread::spawn(move || {
            let _ = child.wait();
        });
        let (input, pending) = channel::channel::<Vec<u8>>();
        std::thread::spawn(move || {
            for bytes in pending {
                if writer
                    .write_all(&bytes)
                    .and_then(|_| writer.flush())
                    .is_err()
                {
                    break;
                }
            }
        });
        // Bound output so a noisy command cannot outgrow the UI's consumer.
        let (mut output, receiver) = mpsc::channel(32);
        std::thread::spawn(move || {
            let mut bytes = [0; 16384];
            loop {
                match reader.read(&mut bytes) {
                    Ok(0) => break,
                    Ok(n) => {
                        if futures::executor::block_on(output.send(bytes[..n].to_vec())).is_err() {
                            break;
                        }
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
                    Err(_) => break,
                }
            }
        });
        Ok((
            Self {
                master: pair.master,
                input,
                killer,
            },
            receiver,
        ))
    }
}

pub struct Terminal {
    emulator: Emulator,
    shell: Option<Shell>,
    focus: FocusHandle,
    geometry: Option<GridGeometry>,
    selecting: bool,
    scroll_remainder: f32,
    cwd: std::path::PathBuf,
    status: Option<String>,
    _pump: Option<Task<()>>,
}

impl Terminal {
    pub fn new(cwd: &Path, cx: &mut Context<Self>) -> Self {
        let mut this = Self {
            emulator: Emulator::new(80, 24),
            shell: None,
            focus: cx.focus_handle(),
            geometry: None,
            selecting: false,
            scroll_remainder: 0.,
            cwd: cwd.to_path_buf(),
            status: None,
            _pump: None,
        };
        match Shell::open(cwd) {
            Ok((shell, mut output)) => {
                this.shell = Some(shell);
                this._pump = Some(cx.spawn(async move |this, cx| {
                    while let Some(bytes) = output.next().await {
                        if this
                            .update(cx, |this, cx| {
                                let reply = this.emulator.feed(&bytes);
                                this.write(reply);
                                cx.notify();
                            })
                            .is_err()
                        {
                            return;
                        }
                    }
                    let _ = this.update(cx, |this, cx| {
                        this.shell = None;
                        this.status = Some("Shell exited".into());
                        cx.notify();
                    });
                }));
            }
            Err(error) => this.status = Some(format!("Could not start terminal: {error}")),
        }
        this
    }

    fn write(&self, bytes: Vec<u8>) {
        if !bytes.is_empty()
            && let Some(shell) = &self.shell
        {
            let _ = shell.input.send(bytes);
        }
    }

    fn key(&mut self, event: &gpui::KeyDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        let key = &event.keystroke;
        if let Some(bytes) = keystroke_bytes(key, self.emulator.app_cursor_mode()) {
            self.emulator.clear_selection();
            self.emulator.scroll_to_bottom();
            self.write(bytes);
            cx.stop_propagation();
            cx.notify();
        }
    }

    fn copy(&mut self, _: &input::Copy, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(text) = self.emulator.selection_text() {
            cx.write_to_clipboard(ClipboardItem::new_string(text));
        }
    }

    fn paste(&mut self, _: &input::Paste, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
            self.emulator.scroll_to_bottom();
            self.write(view::paste_bytes(
                &text,
                self.emulator.bracketed_paste_mode(),
            ));
            cx.notify();
        }
    }

    fn select(&mut self, position: gpui::Point<gpui::Pixels>, start: bool) {
        let Some(grid) = self.geometry else {
            return;
        };
        let hit = view::cell_at(
            f32::from(position.x - grid.origin.x),
            f32::from(position.y - grid.origin.y),
            grid.cell_w,
            grid.line_h,
            grid.cols as usize,
            grid.rows as usize,
        );
        let point = self.emulator.grid_point(hit.row, hit.col);
        if start {
            self.emulator
                .start_selection(SelectionType::Simple, point, hit.side);
        } else {
            self.emulator.update_selection(point, hit.side);
        }
    }
}

impl Focusable for Terminal {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Render for Terminal {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = Theme::of(cx).clone();
        let this = cx.weak_entity();
        let grid = TerminalElement::new(
            move |geometry, cx| {
                this.update(cx, |this, _| {
                    if this.emulator.cols() != geometry.cols as usize
                        || this.emulator.rows() != geometry.rows as usize
                    {
                        this.emulator.resize(geometry.cols, geometry.rows);
                        if let Some(shell) = &this.shell {
                            let _ = shell.master.resize(PtySize {
                                rows: geometry.rows,
                                cols: geometry.cols,
                                pixel_width: 0,
                                pixel_height: 0,
                            });
                        }
                    }
                    this.geometry = Some(geometry);
                    GridSnapshot {
                        lines: this.emulator.lines(),
                        cursor: this.emulator.cursor(),
                    }
                })
                .ok()
            },
            self.focus.is_focused(window),
        )
        .with_text_size(typography::terminal_size(cx));
        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(view::terminal_panel_bg(&theme))
            .child(
                div()
                    .h(px(30.))
                    .flex_none()
                    .px(px(12.))
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .text_style(TextStyle::Caption)
                    .text_color(theme.text_muted)
                    .child(icons::icon(icons::development::Terminal).size(px(14.)))
                    .child(
                        div()
                            .flex_1()
                            .child(self.status.clone().unwrap_or_else(|| "Terminal".into())),
                    )
                    .when(self.status.is_some(), |header| {
                        header.child(
                            div()
                                .id("terminal-restart")
                                .px(px(6.))
                                .cursor_pointer()
                                .hover(|s| s.text_color(theme.text))
                                .on_click(cx.listener(|this, _, window, cx| {
                                    let cwd = this.cwd.clone();
                                    *this = Self::new(&cwd, cx);
                                    window.focus(&this.focus, cx);
                                    cx.notify();
                                }))
                                .child("Restart"),
                        )
                    })
                    .child(
                        div()
                            .id("terminal-hide")
                            .size(px(24.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(4.))
                            .cursor_pointer()
                            .hover(|s| s.bg(theme.element_hover))
                            .tooltip(|window, cx| Tooltip::text("Hide terminal", window, cx))
                            .on_click(|_, window, cx| {
                                window.dispatch_action(
                                    Box::new(crate::view::root::ToggleTerminal),
                                    cx,
                                )
                            })
                            .child(icons::icon(icons::notifications::X).size(px(14.))),
                    ),
            )
            .child(
                div()
                    .id("session-terminal")
                    .flex_1()
                    .min_h_0()
                    .overflow_hidden()
                    .key_context(CONTEXT)
                    .track_focus(&self.focus)
                    .on_key_down(cx.listener(Self::key))
                    .on_action(|_: &IncreaseTextSize, _, cx| typography::zoom_terminal(1., cx))
                    .on_action(|_: &DecreaseTextSize, _, cx| typography::zoom_terminal(-1., cx))
                    .on_action(|_: &ResetTextSize, _, cx| typography::reset_terminal_zoom(cx))
                    .on_action(cx.listener(Self::copy))
                    .on_action(cx.listener(Self::paste))
                    .on_mouse_down(
                        gpui::MouseButton::Left,
                        cx.listener(|this, event: &gpui::MouseDownEvent, window, cx| {
                            window.focus(&this.focus, cx);
                            this.selecting = true;
                            this.select(event.position, true);
                            cx.notify();
                        }),
                    )
                    .on_mouse_move(cx.listener(|this, event: &gpui::MouseMoveEvent, _, cx| {
                        if this.selecting {
                            this.select(event.position, false);
                            cx.notify();
                        }
                    }))
                    .on_mouse_up(
                        gpui::MouseButton::Left,
                        cx.listener(|this, _, _, _| this.selecting = false),
                    )
                    .on_mouse_up_out(
                        gpui::MouseButton::Left,
                        cx.listener(|this, _, _, _| this.selecting = false),
                    )
                    .on_scroll_wheel(cx.listener(|this, event: &gpui::ScrollWheelEvent, _, cx| {
                        let line_h = this.geometry.map_or(20., |grid| grid.line_h);
                        let delta = f32::from(event.delta.pixel_delta(px(line_h)).y) / line_h;
                        this.scroll_remainder += delta;
                        let lines = this.scroll_remainder.trunc() as i32;
                        this.scroll_remainder -= lines as f32;
                        this.emulator.scroll(lines);
                        cx.stop_propagation();
                        cx.notify();
                    }))
                    .child(grid),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn shell() -> (Shell, mpsc::Receiver<Vec<u8>>) {
        let mut command = CommandBuilder::new("/bin/sh");
        command.arg("-i");
        command.env("ENV", "/dev/null");
        Shell::open_command(Path::new("/private/tmp"), command).expect("open PTY")
    }

    fn collect(mut output: mpsc::Receiver<Vec<u8>>) -> channel::Receiver<Vec<u8>> {
        let (tx, rx) = channel::channel();
        std::thread::spawn(move || {
            let bytes = futures::executor::block_on(async {
                let mut bytes = Vec::new();
                while let Some(chunk) = output.next().await {
                    bytes.extend(chunk);
                }
                bytes
            });
            let _ = tx.send(bytes);
        });
        rx
    }

    #[test]
    fn shell_has_session_directory_terminal_environment_and_resizes() {
        let (shell, output) = shell();
        let output = collect(output);
        shell
            .master
            .resize(PtySize {
                rows: 37,
                cols: 101,
                pixel_width: 0,
                pixel_height: 0,
            })
            .unwrap();
        shell
            .input
            .send(b"printf '\\nCWD:%s\\nTERM:%s\\n' \"$PWD\" \"$TERM\"; stty size; exit\n".to_vec())
            .unwrap();
        let bytes = output
            .recv_timeout(Duration::from_secs(10))
            .expect("shell exits and closes output");
        let text = String::from_utf8_lossy(&bytes);
        assert!(text.contains("CWD:/private/tmp\r\n"), "{text}");
        assert!(text.contains("TERM:xterm-256color\r\n"), "{text}");
        assert!(text.contains("37 101\r\n"), "{text}");
    }

    #[test]
    fn dropping_shell_closes_its_output() {
        let (shell, output) = shell();
        let output = collect(output);
        drop(shell);
        output
            .recv_timeout(Duration::from_secs(10))
            .expect("dropping a session ends its shell");
    }
}
