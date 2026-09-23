//! A session's shell: the PTY lives as long as this entity, even while hidden.

use crate::model::typography;
use bezel::{
    gpui::{
        self, App, ClipboardItem, Context, Entity, EventEmitter, FocusHandle, Focusable,
        KeyBinding, Render, Subscription, Task, Window, div, prelude::*, px,
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
    emulator::{Emulator, HOLD_TIMEOUT, KeyboardMode, SelectionType},
    view::{
        self, GridGeometry, GridSnapshot, Images, KeyEvent, MouseAction, MouseButton,
        SELECTION_DRAG_THRESHOLD, TerminalElement,
    },
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
pub fn keystroke_bytes(
    key: &gpui::Keystroke,
    mode: KeyboardMode,
    event: KeyEvent,
) -> Option<Vec<u8>> {
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
        mode,
        event,
    )
}

/// Dropping the panel's owner terminates the shell; a waiter reaps it off-thread.
struct Shell {
    pid: Option<u32>,
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
        // What a program reads to know which terminal it is talking to.
        command.env("TERM_PROGRAM", "cydonia");
        command.env("TERM_PROGRAM_VERSION", env!("CARGO_PKG_VERSION"));
        // The grid draws kitty graphics, and this is the variable programs
        // sniff for them — the emulator also answers the protocol's own `a=q`
        // query, which the ones that ask get a truthful answer from. TERM stays
        // `xterm-256color`: `xterm-kitty` is a terminfo entry that only exists
        // on a machine with kitty installed.
        command.env("KITTY_WINDOW_ID", "1");
        let mut child = pair.slave.spawn_command(command)?;
        let pid = child.process_id();
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
                pid,
                master: pair.master,
                input,
                killer,
            },
            receiver,
        ))
    }
}

#[cfg(target_os = "macos")]
fn process_directory(pid: u32) -> Option<std::path::PathBuf> {
    use std::os::unix::ffi::OsStringExt;
    let mut info = std::mem::MaybeUninit::<libc::proc_vnodepathinfo>::uninit();
    let size = std::mem::size_of_val(&info) as i32;
    // The kernel initializes the full structure on a successful, full-size read.
    let read = unsafe {
        libc::proc_pidinfo(
            pid as i32,
            libc::PROC_PIDVNODEPATHINFO,
            0,
            info.as_mut_ptr().cast(),
            size,
        )
    };
    if read != size {
        return None;
    }
    let info = unsafe { info.assume_init() };
    let bytes: Vec<u8> = info
        .pvi_cdir
        .vip_path
        .iter()
        .flatten()
        .map(|byte| *byte as u8)
        .take_while(|byte| *byte != 0)
        .collect();
    if bytes.is_empty() {
        return None;
    }
    Some(std::ffi::OsString::from_vec(bytes).into())
}

#[cfg(target_os = "linux")]
fn process_directory(pid: u32) -> Option<std::path::PathBuf> {
    std::fs::read_link(format!("/proc/{pid}/cwd")).ok()
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn process_directory(_: u32) -> Option<std::path::PathBuf> {
    None
}

fn directory_label(directory: &Path) -> String {
    directory
        .file_name()
        .unwrap_or(directory.as_os_str())
        .to_string_lossy()
        .into_owned()
}

pub struct Terminal {
    pub(crate) directory: std::path::PathBuf,
    emulator: Emulator,
    /// Decoded kitty images, cached across frames beside the emulator holding
    /// the bytes they came from.
    images: Images,
    shell: Option<Shell>,
    focus: FocusHandle,
    geometry: Option<GridGeometry>,
    /// Where the left button went down, until the pointer has travelled
    /// [`SELECTION_DRAG_THRESHOLD`] and the press becomes a selection.
    pressed: Option<gpui::Point<gpui::Pixels>>,
    selecting: bool,
    scroll_remainder: f32,
    status: Option<String>,
    /// Pending release of a render hold, armed while one is on.
    hold: Option<Task<()>>,
    _pump: Option<Task<()>>,
}

impl Terminal {
    pub fn new(cwd: &Path, cx: &mut Context<Self>) -> Self {
        let mut this = Self {
            directory: cwd.canonicalize().unwrap_or_else(|_| cwd.to_path_buf()),
            emulator: Emulator::new(80, 24),
            images: Images::new(),
            shell: None,
            focus: cx.focus_handle(),
            geometry: None,
            pressed: None,
            selecting: false,
            scroll_remainder: 0.,
            status: None,
            hold: None,
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
                                this.arm_hold_release(cx);
                                if let Some(directory) = this
                                    .shell
                                    .as_ref()
                                    .and_then(|shell| shell.pid)
                                    .and_then(process_directory)
                                    && directory != this.directory
                                {
                                    this.directory = directory;
                                    cx.emit(DirectoryChanged);
                                }
                                cx.notify();
                            })
                            .is_err()
                        {
                            return;
                        }
                    }
                    let _ = this.update(cx, |this, cx| {
                        this.shell = None;
                        cx.emit(Exited);
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
        let kind = if event.is_held {
            KeyEvent::Repeat
        } else {
            KeyEvent::Press
        };
        self.send_key(&event.keystroke, kind, cx);
    }

    fn key_up(&mut self, event: &gpui::KeyUpEvent, _: &mut Window, cx: &mut Context<Self>) {
        self.send_key(&event.keystroke, KeyEvent::Release, cx);
    }

    /// Hand one key to the program. A release only encodes under the kitty
    /// keyboard protocol, so off it this is a no-op.
    fn send_key(&mut self, key: &gpui::Keystroke, event: KeyEvent, cx: &mut Context<Self>) {
        let Some(bytes) = keystroke_bytes(key, self.emulator.keyboard_mode(), event) else {
            return;
        };
        // Letting a key go is not the user typing: it must not drop the
        // selection or pull the view back to the live bottom.
        if !matches!(event, KeyEvent::Release) {
            self.emulator.clear_selection();
            self.emulator.scroll_to_bottom();
        }
        self.write(bytes);
        cx.stop_propagation();
        cx.notify();
    }

    /// Release a render hold the program never ended. The emulator has no
    /// clock, so a program that sets mode 2026 and dies would otherwise leave
    /// the grid on its half-drawn frame for good.
    ///
    /// Re-armed on every read: a program still writing inside its frame is
    /// alive, and this is here for one that is not.
    fn arm_hold_release(&mut self, cx: &mut Context<Self>) {
        if !self.emulator.render_hold() {
            self.hold = None;
            return;
        }
        self.hold = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(HOLD_TIMEOUT).await;
            let _ = this.update(cx, |this, cx| {
                this.emulator.release_hold();
                cx.notify();
            });
        }));
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

    /// Which cell a window position landed on, or `None` before the grid has
    /// been measured.
    fn cell(&self, position: gpui::Point<gpui::Pixels>) -> Option<view::CellHit> {
        let grid = self.geometry?;
        Some(view::cell_at(
            f32::from(position.x - grid.origin.x),
            f32::from(position.y - grid.origin.y),
            grid.cell_w,
            grid.line_h,
            grid.cols as usize,
            grid.rows as usize,
        ))
    }

    /// Offer a pointer event to the running program. `true` means it took it,
    /// and the host should leave its own selection and scrollback alone.
    fn report(
        &mut self,
        action: MouseAction,
        position: gpui::Point<gpui::Pixels>,
        modifiers: &gpui::Modifiers,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(hit) = self.cell(position) else {
            return false;
        };
        let Some(bytes) = view::mouse_bytes(
            action,
            hit.row,
            hit.col,
            modifiers,
            self.emulator.mouse_mode(),
        ) else {
            return false;
        };
        self.write(bytes);
        cx.notify();
        true
    }

    fn select(&mut self, position: gpui::Point<gpui::Pixels>, start: bool) {
        let Some(hit) = self.cell(position) else {
            return;
        };
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
                    // Only this callback has the measured cell, which is what
                    // sizes a kitty image in rows and columns.
                    this.emulator
                        .set_cell_size(geometry.cell_w, geometry.line_h);
                    GridSnapshot {
                        lines: this.emulator.lines(),
                        cursor: this.emulator.cursor(),
                        images: this.images.placed(&this.emulator),
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
            .when_some(self.status.clone(), |panel, status| {
                panel.child(div().px(px(12.)).text_color(theme.text_muted).child(status))
            })
            .child(
                div()
                    .id("session-terminal")
                    .flex_1()
                    .min_h_0()
                    .overflow_hidden()
                    .key_context(CONTEXT)
                    .track_focus(&self.focus)
                    .on_key_down(cx.listener(Self::key))
                    .on_key_up(cx.listener(Self::key_up))
                    .on_action(|_: &IncreaseTextSize, _, cx| typography::zoom_terminal(1., cx))
                    .on_action(|_: &DecreaseTextSize, _, cx| typography::zoom_terminal(-1., cx))
                    .on_action(|_: &ResetTextSize, _, cx| typography::reset_terminal_zoom(cx))
                    .on_action(cx.listener(Self::copy))
                    .on_action(cx.listener(Self::paste))
                    .on_mouse_down(
                        gpui::MouseButton::Left,
                        cx.listener(|this, event: &gpui::MouseDownEvent, window, cx| {
                            window.focus(&this.focus, cx);
                            if this.report(
                                MouseAction::Press(MouseButton::Left),
                                event.position,
                                &event.modifiers,
                                cx,
                            ) {
                                return;
                            }
                            // The press itself is not yet a selection: the one
                            // that focuses the panel would otherwise take a
                            // cell with it.
                            this.pressed = Some(event.position);
                            this.emulator.clear_selection();
                            cx.notify();
                        }),
                    )
                    .on_mouse_down(
                        gpui::MouseButton::Right,
                        cx.listener(|this, event: &gpui::MouseDownEvent, _, cx| {
                            this.report(
                                MouseAction::Press(MouseButton::Right),
                                event.position,
                                &event.modifiers,
                                cx,
                            );
                        }),
                    )
                    .on_mouse_down(
                        gpui::MouseButton::Middle,
                        cx.listener(|this, event: &gpui::MouseDownEvent, _, cx| {
                            this.report(
                                MouseAction::Press(MouseButton::Middle),
                                event.position,
                                &event.modifiers,
                                cx,
                            );
                        }),
                    )
                    .on_mouse_move(cx.listener(|this, event: &gpui::MouseMoveEvent, _, cx| {
                        if !this.selecting {
                            let held = match event.pressed_button {
                                Some(gpui::MouseButton::Left) => Some(MouseButton::Left),
                                Some(gpui::MouseButton::Right) => Some(MouseButton::Right),
                                Some(gpui::MouseButton::Middle) => Some(MouseButton::Middle),
                                _ => None,
                            };
                            if this.report(
                                MouseAction::Motion(held),
                                event.position,
                                &event.modifiers,
                                cx,
                            ) {
                                return;
                            }
                        }
                        if let Some(origin) = this.pressed {
                            let travel = event.position - origin;
                            if f32::from(travel.x).abs().max(f32::from(travel.y).abs())
                                < SELECTION_DRAG_THRESHOLD
                            {
                                return;
                            }
                            this.pressed = None;
                            this.selecting = true;
                            this.select(origin, true);
                        }
                        if this.selecting {
                            this.select(event.position, false);
                            cx.notify();
                        }
                    }))
                    .on_mouse_up(
                        gpui::MouseButton::Left,
                        cx.listener(|this, event: &gpui::MouseUpEvent, _, cx| {
                            this.pressed = None;
                            if !this.selecting {
                                this.report(
                                    MouseAction::Release(MouseButton::Left),
                                    event.position,
                                    &event.modifiers,
                                    cx,
                                );
                            }
                            this.selecting = false;
                        }),
                    )
                    .on_mouse_up(
                        gpui::MouseButton::Right,
                        cx.listener(|this, event: &gpui::MouseUpEvent, _, cx| {
                            this.report(
                                MouseAction::Release(MouseButton::Right),
                                event.position,
                                &event.modifiers,
                                cx,
                            );
                        }),
                    )
                    .on_mouse_up(
                        gpui::MouseButton::Middle,
                        cx.listener(|this, event: &gpui::MouseUpEvent, _, cx| {
                            this.report(
                                MouseAction::Release(MouseButton::Middle),
                                event.position,
                                &event.modifiers,
                                cx,
                            );
                        }),
                    )
                    .on_mouse_up_out(
                        gpui::MouseButton::Left,
                        cx.listener(|this, _, _, _| {
                            this.pressed = None;
                            this.selecting = false;
                        }),
                    )
                    .on_scroll_wheel(cx.listener(|this, event: &gpui::ScrollWheelEvent, _, cx| {
                        let line_h = this.geometry.map_or(20., |grid| grid.line_h);
                        let delta = f32::from(event.delta.pixel_delta(px(line_h)).y) / line_h;
                        this.scroll_remainder += delta;
                        let lines = this.scroll_remainder.trunc() as i32;
                        this.scroll_remainder -= lines as f32;
                        if lines != 0
                            && this.report(
                                MouseAction::Scroll {
                                    up: lines > 0,
                                    lines: lines.unsigned_abs() as usize,
                                },
                                event.position,
                                &event.modifiers,
                                cx,
                            )
                        {
                            cx.stop_propagation();
                            return;
                        }
                        this.emulator.scroll(lines);
                        cx.stop_propagation();
                        cx.notify();
                    }))
                    .child(grid),
            )
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/terminal.rs"]
mod tests;

pub(crate) struct DirectoryChanged;
impl EventEmitter<DirectoryChanged> for Terminal {}

pub(crate) struct Exited;
impl EventEmitter<Exited> for Terminal {}

pub struct Empty;

struct Tab {
    id: usize,
    terminal: Entity<Terminal>,
    _exit: Subscription,
    _directory: Subscription,
}

/// Where a shell should start, asked of the window as the tab is made.
type Cwd = Box<dyn Fn(&App) -> Option<std::path::PathBuf>>;

pub struct TerminalPanel {
    cwd: std::path::PathBuf,
    /// Where the next tab opens, asked as the tab is made. The panel stays
    /// open across everything the window moves to, so the directory it was
    /// first opened at is not where a shell started now belongs.
    next_cwd: Cwd,
    tabs: Vec<Tab>,
    active: usize,
    next_id: usize,
}

impl EventEmitter<Empty> for TerminalPanel {}

impl TerminalPanel {
    /// `cwd` is where the first tab opens; `next_cwd` is asked for every one
    /// after it. Two arguments because the first is made while the caller is
    /// still mid-update, and cannot read itself back.
    pub fn new(
        cwd: &Path,
        next_cwd: impl Fn(&App) -> Option<std::path::PathBuf> + 'static,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let mut panel = Self {
            cwd: cwd.canonicalize().unwrap_or_else(|_| cwd.to_path_buf()),
            next_cwd: Box::new(next_cwd),
            tabs: Vec::new(),
            active: 0,
            next_id: 1,
        };
        panel.add(window, cx);
        panel
    }

    fn add(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let id = self.next_id;
        self.next_id += 1;
        let cwd = match self.tabs.is_empty() {
            true => self.cwd.clone(),
            false => (self.next_cwd)(cx)
                .map(|cwd| cwd.canonicalize().unwrap_or(cwd))
                .unwrap_or_else(|| self.cwd.clone()),
        };
        let terminal = cx.new(|cx| Terminal::new(&cwd, cx));
        let exit = cx.subscribe_in(&terminal, window, move |this, _, _: &Exited, window, cx| {
            this.close(id, window, cx);
        });
        window.focus(&terminal.focus_handle(cx), cx);
        let directory = cx.subscribe(&terminal, |_, _, _: &DirectoryChanged, cx| cx.notify());
        self.tabs.push(Tab {
            id,
            terminal,
            _exit: exit,
            _directory: directory,
        });
        self.active = id;
        cx.notify();
    }

    /// Step to the tab `step` along, wrapping at the ends, with the focus — the
    /// panel's half of [`super::panel::NextTab`].
    fn cycle(&mut self, step: isize, window: &mut Window, cx: &mut Context<Self>) {
        if self.tabs.len() < 2 {
            return;
        }
        let at = self
            .tabs
            .iter()
            .position(|tab| tab.id == self.active)
            .unwrap_or(0);
        let count = self.tabs.len() as isize;
        let next = (at as isize + step).rem_euclid(count) as usize;
        let Some(tab) = self.tabs.get(next) else {
            return;
        };
        self.active = tab.id;
        window.focus(&tab.terminal.focus_handle(cx), cx);
        cx.notify();
    }

    fn close(&mut self, id: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(index) = self.tabs.iter().position(|tab| tab.id == id) else {
            return;
        };
        let focused = self.tabs[index]
            .terminal
            .focus_handle(cx)
            .is_focused(window);
        self.tabs.remove(index);
        if self.tabs.is_empty() {
            cx.emit(Empty);
        } else if self.active == id {
            let next = &self.tabs[index.min(self.tabs.len() - 1)];
            self.active = next.id;
            if focused {
                window.focus(&next.terminal.focus_handle(cx), cx);
            }
        }
        cx.notify();
    }
}

impl Focusable for TerminalPanel {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.tabs
            .iter()
            .find(|tab| tab.id == self.active)
            .expect("terminal panel has an active tab")
            .terminal
            .focus_handle(cx)
    }
}

impl Render for TerminalPanel {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = Theme::of(cx).clone();
        div()
            .size_full()
            .flex()
            .flex_col()
            .key_context("BottomTerminalPanel")
            .on_action(
                cx.listener(|this, _: &super::panel::NewTerminal, window, cx| {
                    this.add(window, cx);
                }),
            )
            .on_action(
                cx.listener(|this, _: &crate::view::menubar::CloseWindow, window, cx| {
                    this.close(this.active, window, cx);
                }),
            )
            .on_action(cx.listener(|this, _: &super::panel::CloseTab, window, cx| {
                this.close(this.active, window, cx);
            }))
            .on_action(cx.listener(|this, _: &super::panel::NextTab, window, cx| {
                this.cycle(1, window, cx);
            }))
            .on_action(cx.listener(|this, _: &super::panel::PrevTab, window, cx| {
                this.cycle(-1, window, cx);
            }))
            .bg(crate::view::root::content_bg(&theme))
            .child(
                div()
                    .h(px(40.))
                    .flex_none()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .px(px(8.))
                    .text_style(TextStyle::Body)
                    .text_color(theme.text_muted)
                    .child(
                        div()
                            .id("terminal-tabs")
                            .min_w_0()
                            .flex()
                            .gap(px(4.))
                            .overflow_x_scroll()
                            .children(self.tabs.iter().map(|tab| {
                                let id = tab.id;
                                let directory = directory_label(&tab.terminal.read(cx).directory);
                                div()
                                    .id(("terminal-tab", id))
                                    .flex_none()
                                    .flex()
                                    .items_center()
                                    .gap(px(10.))
                                    .px(px(10.))
                                    .w(px(156.))
                                    .h(px(28.))
                                    .rounded(px(10.))
                                    .cursor_pointer()
                                    .hover(|tab| tab.text_color(theme.text))
                                    .when(self.active == id, |tab| {
                                        tab.bg(theme.element_hover).text_color(theme.text)
                                    })
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        this.active = id;
                                        window.focus(&this.focus_handle(cx), cx);
                                        cx.notify();
                                    }))
                                    .child(
                                        icons::icon(icons::development::Terminal)
                                            .size(px(14.))
                                            .flex_none()
                                            .text_color(if self.active == id {
                                                theme.text
                                            } else {
                                                theme.text_muted
                                            }),
                                    )
                                    .child(
                                        div()
                                            .flex_1()
                                            .min_w_0()
                                            .truncate()
                                            .child(directory.clone()),
                                    )
                                    .child(
                                        div()
                                            .id(("terminal-close", id))
                                            .size(px(18.))
                                            .flex_none()
                                            .rounded(px(4.))
                                            .hover(|button| button.bg(theme.element_hover))
                                            .flex()
                                            .items_center()
                                            .justify_center()
                                            .tooltip(|window, cx| {
                                                Tooltip::text("Close terminal", window, cx)
                                            })
                                            .on_click(cx.listener(move |this, _, window, cx| {
                                                cx.stop_propagation();
                                                this.close(id, window, cx);
                                            }))
                                            .child(
                                                icons::icon(icons::notifications::X)
                                                    .size(px(12.))
                                                    .text_color(theme.text_muted),
                                            ),
                                    )
                            })),
                    )
                    .child(
                        div()
                            .id("terminal-add")
                            .size(px(24.))
                            .flex_none()
                            .flex()
                            .items_center()
                            .justify_center()
                            .cursor_pointer()
                            .rounded(px(4.))
                            .hover(|s| s.bg(theme.element_hover))
                            .tooltip(|window, cx| Tooltip::text("New terminal", window, cx))
                            .on_click(cx.listener(|this, _, window, cx| this.add(window, cx)))
                            .child(
                                icons::icon(icons::math::Plus)
                                    .size(px(14.))
                                    .text_color(theme.text_muted),
                            ),
                    )
                    .child(div().flex_1())
                    .child(
                        div()
                            .id("terminal-hide")
                            .flex_none()
                            .size(px(24.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .cursor_pointer()
                            .rounded(px(4.))
                            .hover(|s| s.bg(theme.element_hover))
                            .tooltip(|window, cx| Tooltip::text("Hide terminal panel", window, cx))
                            .on_click(|_, window, cx| {
                                window.dispatch_action(
                                    Box::new(crate::view::root::ToggleTerminal),
                                    cx,
                                )
                            })
                            .child(
                                icons::icon(icons::arrows::ChevronDown)
                                    .size(px(14.))
                                    .text_color(theme.text_muted),
                            ),
                    ),
            )
            .child(
                div().flex_1().min_h_0().children(
                    self.tabs
                        .iter()
                        .find(|tab| tab.id == self.active)
                        .map(|tab| tab.terminal.clone()),
                ),
            )
            .children(
                self.tabs
                    .iter()
                    .find(|tab| tab.id == self.active)
                    .map(|tab| super::status::terminal(&tab.terminal.read(cx).directory, &theme)),
            )
    }
}
