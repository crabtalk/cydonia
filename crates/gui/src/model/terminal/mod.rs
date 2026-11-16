use crate::{
    agent::path,
    model::{identity::launch::Interactive, terminal::process::Process},
};
use bezel::gpui::{Context, EventEmitter, Task};
use futures::{StreamExt, channel::mpsc};
use mcp::terminal::{self as bridge, Bootstrap};
use portable_pty::{CommandBuilder, PtySize};
use std::path::{Path, PathBuf};
use terminal::emulator::{Emulator, HOLD_TIMEOUT};

const FEED_LIMIT: usize = 256 * 1024;

pub mod process;

pub struct Buffer {
    pub directory: PathBuf,
    pub emulator: Emulator,
    pub state: State,
    shell: Option<Process>,
    hold: Option<Task<()>>,
    _pump: Option<Task<()>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum State {
    Running,
    Exited { success: bool, code: u32 },
    Failed(String),
    Stopped,
}

impl State {
    pub fn message(&self) -> Option<String> {
        match self {
            Self::Running => None,
            Self::Exited { code, .. } => Some(format!("Process exited ({code}).")),
            Self::Failed(error) => Some(format!("Terminal error: {error}")),
            Self::Stopped => Some("Process stopped.".into()),
        }
    }

    pub fn failed(&self) -> bool {
        matches!(self, Self::Failed(_) | Self::Exited { success: false, .. })
    }
}

impl Buffer {
    pub fn shell(cwd: &Path, cx: &mut Context<Self>) -> Self {
        Self::open(cwd, Process::shell(cwd), cx)
    }

    pub fn agent(
        cwd: &Path,
        launch: Option<&Interactive>,
        bootstrap: Option<&Bootstrap>,
        cx: &mut Context<Self>,
    ) -> Self {
        let opened = launch
            .ok_or_else(|| anyhow::anyhow!("Set the agent's interactive executable."))
            .and_then(|launch| {
                let mut command = CommandBuilder::new(path::program(&launch.command));
                command.args(&launch.args);
                for (name, value) in &launch.env {
                    command.env(name, value);
                }
                for name in bridge::ENVIRONMENT {
                    command.env_remove(name);
                }
                if let Some(bootstrap) = bootstrap {
                    for (name, value) in &bootstrap.env {
                        command.env(name, value);
                    }
                }
                Process::spawn(cwd, command)
            });
        Self::open(cwd, opened, cx)
    }

    fn open(
        cwd: &Path,
        opened: anyhow::Result<(Process, mpsc::Receiver<Vec<u8>>)>,
        cx: &mut Context<Self>,
    ) -> Self {
        let mut this = Self {
            directory: cwd.canonicalize().unwrap_or_else(|_| cwd.to_path_buf()),
            emulator: Emulator::new(80, 24),
            state: State::Running,
            shell: None,
            hold: None,
            _pump: None,
        };
        match opened {
            Ok((mut shell, mut output)) => {
                let exit = shell.exit.take().unwrap();
                this.shell = Some(shell);
                this._pump = Some(cx.spawn(async move |this, cx| {
                    while let Some(mut bytes) = output.next().await {
                        while bytes.len() < FEED_LIMIT
                            && let Ok(more) = output.try_recv()
                        {
                            bytes.extend_from_slice(&more);
                        }
                        if this.update(cx, |this, cx| this.feed(&bytes, cx)).is_err() {
                            return;
                        }
                    }
                    let state = match exit.await {
                        Ok(Ok(exit)) => State::Exited {
                            success: exit.success(),
                            code: exit.exit_code(),
                        },
                        Ok(Err(error)) => State::Failed(error.to_string()),
                        Err(error) => State::Failed(error.to_string()),
                    };
                    let _ = this.update(cx, |this, cx| {
                        this.state = state;
                        this.shell = None;
                        this.emulator.release_hold();
                        cx.emit(Exited);
                        cx.notify();
                    });
                }));
            }
            Err(error) => this.state = State::Failed(error.to_string()),
        }
        this
    }

    pub fn resize(&mut self, cols: u16, rows: u16) {
        if self.emulator.cols() == cols as usize && self.emulator.rows() == rows as usize {
            return;
        }
        self.emulator.resize(cols, rows);
        if let Some(shell) = &self.shell {
            let _ = shell.master.resize(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            });
        }
    }

    pub fn stop(&mut self, cx: &mut Context<Self>) {
        if self.shell.take().is_some() {
            self.state = State::Stopped;
            self._pump = None;
            self.hold = None;
            self.emulator.release_hold();
            cx.emit(Exited);
            cx.notify();
        }
    }

    pub(crate) fn feed(&mut self, bytes: &[u8], cx: &mut Context<Self>) {
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
                    .and_then(process::directory)
            })
            && directory != self.directory
        {
            self.directory = directory;
            cx.emit(DirectoryChanged);
        }
        cx.notify();
    }

    pub fn write(&self, bytes: Vec<u8>) {
        if !bytes.is_empty()
            && let Some(shell) = &self.shell
        {
            let _ = shell.input.send(bytes);
        }
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
}

pub(crate) struct DirectoryChanged;
impl EventEmitter<DirectoryChanged> for Buffer {}
pub(crate) struct Exited;
impl EventEmitter<Exited> for Buffer {}
