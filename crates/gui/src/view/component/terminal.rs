//! A session's shell: the PTY lives as long as this entity, even while hidden.

use crate::{model::typography, view::keymap};
use bezel::{
    gpui::{
        self, App, ClipboardEntry, ClipboardItem, Context, Edges, Entity, EventEmitter,
        ExternalPaths, FocusHandle, Focusable, Global, KeyBinding, Render, Subscription, Task,
        Window, div, prelude::*, px,
    },
    motion,
    theme::{TextStyle, Theme, Typeset},
    ui::{AppExt as _, icons, input, tabs, tooltip::Tooltip},
};
use futures::{SinkExt, StreamExt, channel::mpsc};
use portable_pty::{ChildKiller, CommandBuilder, MasterPty, PtySize, native_pty_system};
use std::{
    io::{Read, Write},
    path::Path,
    sync::mpsc as channel,
    time::Duration,
};
use terminal::{
    emulator::{CursorShape, CursorStyle, Emulator, HOLD_TIMEOUT, KeyboardMode, SelectionType},
    view::{
        self, Batched, GridGeometry, GridSnapshot, Images, KeyEvent, MouseAction, MouseButton,
        OUTPUT_BATCH_MS, OutputBatch, SELECTION_DRAG_THRESHOLD, TerminalElement,
    },
};

const CONTEXT: &str = "CydoniaTerminal";

/// Half the cursor's blink period, the text caret's own.
const BLINK: Duration = Duration::from_millis(500);

/// Whether terminals draw the app's caret instead of the program's cursor.
struct CaretOverride(bool);

impl Global for CaretOverride {}

pub fn set_caret_override(on: bool, cx: &mut App) {
    cx.set_global(CaretOverride(on));
    cx.refresh_windows();
}

/// The app's caret as a terminal cursor, when terminals take it.
fn caret_override(cx: &App) -> Option<CursorStyle> {
    if !cx.try_global::<CaretOverride>().is_some_and(|on| on.0) {
        return None;
    }
    let shape = match cx.caret_shape() {
        input::CaretShape::Bar => CursorShape::Beam,
        input::CaretShape::Block => CursorShape::Block,
        input::CaretShape::Underline => CursorShape::Underline,
    };
    Some(CursorStyle {
        shape,
        blinking: cx.caret_blink(),
    })
}

/// How often a selection drag held past the grid's top or bottom edge
/// scrolls the scrollback.
const EDGE_SCROLL_TICK: Duration = Duration::from_millis(50);
/// Most lines one edge-scroll tick moves, however far past the edge the
/// pointer is.
const EDGE_SCROLL_MAX_LINES: i32 = 12;

/// Paths as a shell reads them: each single-quoted where it needs to be,
/// joined by spaces, with a trailing space so the next word starts clean.
fn shell_words(paths: &ExternalPaths) -> String {
    let mut out = String::new();
    for path in paths.paths() {
        let path = path.to_string_lossy();
        let plain = !path.is_empty()
            && path
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || "/._-+,:@%=".contains(c));
        if plain {
            out.push_str(&path);
        } else {
            out.push('\'');
            out.push_str(&path.replace('\'', "'\\''"));
            out.push('\'');
        }
        out.push(' ');
    }
    out
}

gpui::actions!(
    cydonia_terminal,
    [IncreaseTextSize, DecreaseTextSize, ResetTextSize]
);

pub fn bindings() -> Vec<KeyBinding> {
    vec![
        // Off macOS `ctrl-c` and `ctrl-v` are the shell's.
        KeyBinding::new(
            keymap::platform("cmd-c", "ctrl-shift-c"),
            input::Copy,
            Some(CONTEXT),
        ),
        KeyBinding::new(
            keymap::platform("cmd-v", "ctrl-shift-v"),
            input::Paste,
            Some(CONTEXT),
        ),
        KeyBinding::new("secondary-=", IncreaseTextSize, Some(CONTEXT)),
        KeyBinding::new("secondary-+", IncreaseTextSize, Some(CONTEXT)),
        KeyBinding::new("secondary-shift-=", IncreaseTextSize, Some(CONTEXT)),
        KeyBinding::new("secondary--", DecreaseTextSize, Some(CONTEXT)),
        KeyBinding::new("secondary-0", ResetTextSize, Some(CONTEXT)),
    ]
}

/// Option sends Meta using the base key, not its macOS alternate character.
pub fn keystroke_bytes(
    key: &gpui::Keystroke,
    layout: Option<&gpui::KeyLayout>,
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
        layout,
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
    /// Unit tests get no shell: its reader thread would wake the pump from
    /// outside gpui's test scheduler, which fails the test as nondeterministic.
    #[cfg(test)]
    fn open(_: &Path) -> anyhow::Result<(Self, mpsc::Receiver<Vec<u8>>)> {
        anyhow::bail!("no shell under test")
    }

    #[cfg(not(test))]
    fn open(cwd: &Path) -> anyhow::Result<(Self, mpsc::Receiver<Vec<u8>>)> {
        let shell = std::env::var_os("SHELL").filter(|s| !s.is_empty());
        // `$SHELL` is unset on Windows unless something like Git Bash put it
        // there, and `-l` is a unix shell's flag.
        #[cfg(windows)]
        let command = CommandBuilder::new(shell.unwrap_or_else(|| "powershell.exe".into()));
        #[cfg(not(windows))]
        let command = {
            let mut command = CommandBuilder::new(shell.unwrap_or_else(|| "/bin/zsh".into()));
            command.arg("-l");
            command
        };
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
    /// The pointer while a selection drag is on, wherever it is in the window.
    drag: Option<gpui::Point<gpui::Pixels>>,
    /// Scrolls the scrollback while the drag is held past the top or bottom edge.
    edge_scroll: Option<Task<()>>,
    scroll_remainder: f32,
    status: Option<String>,
    /// Pending release of a render hold, armed while one is on.
    hold: Option<Task<()>>,
    /// PTY output on its way to the emulator.
    batch: OutputBatch,
    /// The batch window's timer, running while the batch is open.
    flush: Option<Task<()>>,
    /// Which half of its blink the cursor is in.
    cursor_on: bool,
    /// The blink, alive only while focused on a blinking cursor.
    blink: Option<Task<()>>,
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
            drag: None,
            edge_scroll: None,
            scroll_remainder: 0.,
            status: None,
            hold: None,
            batch: OutputBatch::default(),
            flush: None,
            cursor_on: true,
            blink: None,
            _pump: None,
        };
        match Shell::open(cwd) {
            Ok((shell, mut output)) => {
                this.shell = Some(shell);
                this._pump = Some(cx.spawn(async move |this, cx| {
                    while let Some(bytes) = output.next().await {
                        if this.update(cx, |this, cx| this.output(bytes, cx)).is_err() {
                            return;
                        }
                    }
                    let _ = this.update(cx, |this, cx| {
                        this.flush = None;
                        if let Some(bytes) = this.batch.tick() {
                            this.feed(&bytes, cx);
                        }
                        this.shell = None;
                        cx.emit(Exited);
                    });
                }));
            }
            Err(error) => this.status = Some(format!("Could not start terminal: {error}")),
        }
        this
    }

    /// Hand one PTY read to the batch, feeding what it gives back.
    fn output(&mut self, bytes: Vec<u8>, cx: &mut Context<Self>) {
        match self.batch.push(bytes) {
            Batched::Open(bytes) => {
                self.feed(&bytes, cx);
                self.schedule_flush(cx);
            }
            Batched::Full(bytes) => self.feed(&bytes, cx),
            Batched::Held => {}
        }
    }

    /// End the batch window: feed what it held and run another window, or
    /// close the batch when it held nothing.
    fn schedule_flush(&mut self, cx: &mut Context<Self>) {
        self.flush = Some(cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(OUTPUT_BATCH_MS))
                .await;
            let _ = this.update(cx, |this, cx| {
                this.flush = None;
                if let Some(bytes) = this.batch.tick() {
                    this.feed(&bytes, cx);
                    this.schedule_flush(cx);
                }
            });
        }));
    }

    fn feed(&mut self, bytes: &[u8], cx: &mut Context<Self>) {
        let reply = self.emulator.feed(bytes);
        self.write(reply);
        self.arm_hold_release(cx);
        // What the shell reports through `OSC 7` or `OSC 9;9` first:
        // PowerShell's `cd` does not move its process directory.
        if let Some(directory) = self
            .emulator
            .directory()
            .map(Path::to_path_buf)
            .or_else(|| {
                self.shell
                    .as_ref()
                    .and_then(|shell| shell.pid)
                    .and_then(process_directory)
            })
            && directory != self.directory
        {
            self.directory = directory;
            cx.emit(DirectoryChanged);
        }
        cx.notify();
    }

    fn start_blink(&mut self, cx: &mut Context<Self>) {
        self.cursor_on = true;
        self.blink = Some(cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(BLINK).await;
                let flipped = this.update(cx, |this, cx| {
                    this.cursor_on = !this.cursor_on;
                    cx.notify();
                });
                if flipped.is_err() {
                    break;
                }
            }
        }));
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
        self.send_key(&event.keystroke, event.layout.as_ref(), kind, cx);
    }

    fn key_up(&mut self, event: &gpui::KeyUpEvent, _: &mut Window, cx: &mut Context<Self>) {
        self.send_key(
            &event.keystroke,
            event.layout.as_ref(),
            KeyEvent::Release,
            cx,
        );
    }

    /// Hand one key to the program. A release only encodes under the kitty
    /// keyboard protocol, so off it this is a no-op.
    fn send_key(
        &mut self,
        key: &gpui::Keystroke,
        layout: Option<&gpui::KeyLayout>,
        event: KeyEvent,
        cx: &mut Context<Self>,
    ) {
        let Some(bytes) = keystroke_bytes(key, layout, self.emulator.keyboard_mode(), event) else {
            return;
        };
        // Letting a key go is not the user typing: it must not drop the
        // selection or pull the view back to the live bottom.
        if !matches!(event, KeyEvent::Release) {
            self.emulator.clear_selection();
            self.emulator.scroll_to_bottom();
            // The next render starts the blink again, lit first.
            self.blink = None;
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
        let Some(item) = cx.read_from_clipboard() else {
            return;
        };
        let files = item.entries().iter().find_map(|entry| match entry {
            ClipboardEntry::ExternalPaths(paths) => Some(shell_words(paths)),
            _ => None,
        });
        if let Some(text) = files.or_else(|| item.text()) {
            self.type_in(&text, cx);
        }
    }

    fn drop_paths(&mut self, paths: &ExternalPaths, window: &mut Window, cx: &mut Context<Self>) {
        window.focus(&self.focus, cx);
        self.type_in(&shell_words(paths), cx);
    }

    fn type_in(&mut self, text: &str, cx: &mut Context<Self>) {
        if text.is_empty() {
            return;
        }
        self.emulator.scroll_to_bottom();
        self.write(view::paste_bytes(
            text,
            self.emulator.bracketed_paste_mode(),
        ));
        cx.notify();
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

    /// Start a selection at `position` when `start` names its granularity,
    /// otherwise extend the one on to it.
    fn select(&mut self, position: gpui::Point<gpui::Pixels>, start: Option<SelectionType>) {
        let Some(hit) = self.cell(position) else {
            return;
        };
        let point = self.emulator.grid_point(hit.row, hit.col);
        if let Some(ty) = start {
            self.emulator.start_selection(ty, point, hit.side);
        } else {
            self.emulator.update_selection(point, hit.side);
        }
    }

    /// Extend the selection to where the drag is, and scroll the scrollback
    /// while it is past the grid's top or bottom edge.
    fn drag_to(&mut self, position: gpui::Point<gpui::Pixels>, cx: &mut Context<Self>) {
        self.drag = Some(position);
        self.select(position, None);
        if self.edge_lines() == 0 {
            self.edge_scroll = None;
        } else if self.edge_scroll.is_none() {
            self.edge_scroll = Some(cx.spawn(async move |this, cx| {
                loop {
                    cx.background_executor().timer(EDGE_SCROLL_TICK).await;
                    let more = this
                        .update(cx, |this, cx| this.edge_scroll_tick(cx))
                        .unwrap_or(false);
                    if !more {
                        return;
                    }
                }
            }));
        }
        cx.notify();
    }

    /// Lines one edge-scroll tick moves: positive up into history, negative
    /// toward live, zero while the drag is over the grid.
    fn edge_lines(&self) -> i32 {
        let (Some(grid), Some(drag)) = (self.geometry, self.drag) else {
            return 0;
        };
        let top = f32::from(grid.origin.y);
        let bottom = top + grid.rows as f32 * grid.line_h;
        let y = f32::from(drag.y);
        let past = if y < top {
            top - y
        } else if y > bottom {
            -(y - bottom)
        } else {
            return 0;
        };
        let lines = 1 + (past.abs() / grid.line_h) as i32;
        lines.min(EDGE_SCROLL_MAX_LINES) * past.signum() as i32
    }

    fn edge_scroll_tick(&mut self, cx: &mut Context<Self>) -> bool {
        let lines = if self.selecting { self.edge_lines() } else { 0 };
        let Some(drag) = self.drag.filter(|_| lines != 0) else {
            self.edge_scroll = None;
            return false;
        };
        self.emulator.scroll(lines);
        self.select(drag, None);
        cx.notify();
        true
    }

    fn end_drag(&mut self) {
        self.pressed = None;
        self.selecting = false;
        self.drag = None;
        self.edge_scroll = None;
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
        let focused = self.focus.is_focused(window);
        self.emulator.set_cursor_override(caret_override(cx));
        let blinking = self
            .emulator
            .cursor()
            .is_some_and(|cursor| cursor.style.blinking);
        if focused && blinking {
            if self.blink.is_none() {
                self.start_blink(cx);
            }
        } else {
            self.blink = None;
            self.cursor_on = true;
        }
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
            focused,
        )
        .with_cursor_on(self.cursor_on)
        .with_text_size(typography::terminal_size(cx))
        .with_content_inset(Edges::all(px(12.0)));
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
                    .on_drop(cx.listener(Self::drop_paths))
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
                            if event.click_count > 1 {
                                this.pressed = None;
                                this.selecting = true;
                                this.drag = Some(event.position);
                                this.select(
                                    event.position,
                                    Some(view::selection_type(event.click_count)),
                                );
                                cx.notify();
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
                        // A selection drag is followed window-wide by the canvas below.
                        if this.selecting {
                            return;
                        }
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
                        if let Some(origin) = this.pressed {
                            let travel = event.position - origin;
                            if f32::from(travel.x).abs().max(f32::from(travel.y).abs())
                                < SELECTION_DRAG_THRESHOLD
                            {
                                return;
                            }
                            this.pressed = None;
                            this.selecting = true;
                            this.select(origin, Some(SelectionType::Simple));
                            this.drag_to(event.position, cx);
                        }
                    }))
                    .on_mouse_up(
                        gpui::MouseButton::Left,
                        cx.listener(|this, event: &gpui::MouseUpEvent, _, cx| {
                            if !this.selecting {
                                this.report(
                                    MouseAction::Release(MouseButton::Left),
                                    event.position,
                                    &event.modifiers,
                                    cx,
                                );
                            }
                            this.end_drag();
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
                        cx.listener(|this, _, _, _| this.end_drag()),
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
                    .child(grid)
                    .when(self.selecting, |panel| {
                        let owner = cx.entity().downgrade();
                        panel.child(
                            gpui::canvas(
                                |_, _, _| {},
                                move |_, _, window, _| {
                                    window.on_mouse_event(
                                        move |event: &gpui::MouseMoveEvent, phase, _, cx| {
                                            if phase != gpui::DispatchPhase::Capture
                                                || event.pressed_button
                                                    != Some(gpui::MouseButton::Left)
                                            {
                                                return;
                                            }
                                            let _ = owner.update(cx, |this, cx| {
                                                if this.selecting {
                                                    this.drag_to(event.position, cx);
                                                }
                                            });
                                        },
                                    );
                                },
                            )
                            .absolute()
                            .size_full(),
                        )
                    }),
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
    /// What each of the strip's ids holds, in no order.
    tabs: Vec<Tab>,
    /// The row: which tabs are open, in what order, and which is in front.
    strip: tabs::Strip<usize>,
    reorder: tabs::Reorder<usize>,
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
            strip: tabs::Strip::new(),
            reorder: tabs::Reorder::new(motion::Painter::of(cx)),
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
        self.strip.open(id);
        cx.notify();
    }

    /// Step to the tab `step` along, wrapping at the ends, with the focus — the
    /// panel's half of [`super::panel::NextTab`].
    fn cycle(&mut self, step: isize, window: &mut Window, cx: &mut Context<Self>) {
        if self.strip.len() < 2 {
            return;
        }
        self.strip.cycle(step);
        window.focus(&self.focus_handle(cx), cx);
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
        self.strip.close(&id);
        if self.strip.is_empty() {
            cx.emit(Empty);
        } else if focused {
            window.focus(&self.focus_handle(cx), cx);
        }
        cx.notify();
    }

    fn active(&self) -> usize {
        self.strip.active().copied().unwrap_or_default()
    }
}

impl Focusable for TerminalPanel {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.tabs
            .iter()
            .find(|tab| Some(&tab.id) == self.strip.active())
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
                    this.close(this.active(), window, cx);
                }),
            )
            .on_action(cx.listener(|this, _: &super::panel::CloseTab, window, cx| {
                this.close(this.active(), window, cx);
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
                        self.reorder
                            .bar(
                                "terminal-tabs",
                                &self.strip,
                                self.strip.tabs().iter().filter_map(|&id| {
                                    let tab = self.tabs.iter().find(|tab| tab.id == id)?;
                                    let directory =
                                        directory_label(&tab.terminal.read(cx).directory);
                                    let label = tabs::Label::new(directory)
                                        .with_icon(icons::development::Terminal);
                                    let state = match self.strip.active() == Some(&id) {
                                        true => tabs::State::Focused,
                                        false => tabs::State::Resting,
                                    };
                                    let key = gpui::SharedString::from(format!("terminal-{id}"));
                                    let tab = tabs::tab(&theme, key.clone(), label, state)
                                        .on_click(cx.listener(move |this, _, window, cx| {
                                            this.strip.activate(&id);
                                            window.focus(&this.focus_handle(cx), cx);
                                            cx.notify();
                                        }))
                                        .child(
                                            tabs::close(&theme, key, tabs::Close::OnHover)
                                                .on_click(cx.listener(
                                                    move |this, _, window, cx| {
                                                        cx.stop_propagation();
                                                        this.close(id, window, cx);
                                                    },
                                                )),
                                        );
                                    Some((id, tab))
                                }),
                            )
                            .on_reorder(cx.listener(|this, moved: &tabs::Move, _, cx| {
                                this.strip.reorder(moved.from, moved.to);
                                cx.notify();
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
                        .find(|tab| Some(&tab.id) == self.strip.active())
                        .map(|tab| tab.terminal.clone()),
                ),
            )
            .children(
                self.tabs
                    .iter()
                    .find(|tab| Some(&tab.id) == self.strip.active())
                    .map(|tab| super::status::terminal(&tab.terminal.read(cx).directory, &theme)),
            )
    }
}
