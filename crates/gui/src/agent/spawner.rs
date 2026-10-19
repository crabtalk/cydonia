//! Where an agent process runs: on the host a project's files are on.
//!
//! [`open`] is the one place a project picks its spawner. The program, its
//! working directory, the environment it inherits and the `PATH` its name is
//! looked up on are all the spawner's host's.

use std::{
    collections::BTreeMap,
    ffi::OsString,
    future::Future,
    io,
    path::{Path, PathBuf},
    pin::Pin,
    process::Stdio,
};
use tokio::{
    io::{AsyncRead, AsyncWrite},
    process::{Child, Command},
};

/// `CREATE_NO_WINDOW` from the Win32 process creation flags.
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// What to start.
pub struct Spec<'a> {
    /// A path, or a bare name looked up on the host's `PATH`.
    pub program: &'a str,
    pub args: &'a [String],
    /// Set over the host's environment.
    pub env: &'a BTreeMap<String, String>,
    pub cwd: &'a Path,
}

/// A started process: its three standard streams and its handle.
pub struct Process {
    pub stdin: Box<dyn AsyncWrite + Send + Unpin>,
    pub stdout: Box<dyn AsyncRead + Send + Unpin>,
    pub stderr: Box<dyn AsyncRead + Send + Unpin>,
    /// Dropping it kills the process.
    pub handle: Box<dyn Handle>,
}

/// A running process, killed when dropped.
pub trait Handle: Send {
    /// Resolves once the process has ended.
    fn ended(&mut self) -> Pin<Box<dyn Future<Output = ()> + Send + '_>>;
}

/// Starts processes on one host.
pub trait Spawner: Send + Sync {
    /// `key` in the environment a process started here inherits.
    fn var(&self, key: &str) -> Option<String>;

    /// Start `spec` with all three standard streams piped.
    fn spawn(&self, spec: &Spec) -> io::Result<Process>;
}

/// The spawner for a project, by its root.
pub fn open(_root: &Path) -> Box<dyn Spawner> {
    Box::new(Local::new())
}

/// This machine. Its `PATH` is this process's as [`super::path::adopt`]
/// leaves it.
pub struct Local {
    path: OsString,
}

impl Local {
    pub fn new() -> Self {
        Self {
            path: std::env::var_os("PATH").unwrap_or_default(),
        }
    }

    /// The file to exec for `command` — see [`super::path::program`].
    fn program(&self, command: &str) -> PathBuf {
        super::path::program(command, &self.path)
    }
}

impl Default for Local {
    fn default() -> Self {
        Self::new()
    }
}

impl Spawner for Local {
    fn var(&self, key: &str) -> Option<String> {
        std::env::var(key).ok()
    }

    fn spawn(&self, spec: &Spec) -> io::Result<Process> {
        let mut command = Command::new(self.program(spec.program));
        command
            .args(spec.args)
            .current_dir(spec.cwd)
            .env("PATH", &self.path)
            .envs(spec.env)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            // Without it the process outlives its `Child`: dropping the
            // handle does not signal it.
            .kill_on_drop(true);
        // A console program started from a GUI process opens a console window
        // of its own on Windows.
        #[cfg(windows)]
        command.creation_flags(CREATE_NO_WINDOW);
        let mut child = command.spawn()?;
        let piped = || io::Error::other("a standard stream was not piped");
        Ok(Process {
            stdin: Box::new(child.stdin.take().ok_or_else(piped)?),
            stdout: Box::new(child.stdout.take().ok_or_else(piped)?),
            stderr: Box::new(child.stderr.take().ok_or_else(piped)?),
            handle: Box::new(child),
        })
    }
}

impl Handle for Child {
    fn ended(&mut self) -> Pin<Box<dyn Future<Output = ()> + Send + '_>> {
        Box::pin(async move {
            let _ = self.wait().await;
        })
    }
}
