//! Where an agent process runs: on the host a project's files are on.
//!
//! [`open`] is the one place a project picks its spawner. The program, its
//! working directory, the environment it inherits and the `PATH` its name is
//! looked up on are all the spawner's host's.

use super::path::{DUMP, Environment, LOGIN, TIMEOUT};
use cupola::{
    exec::{self, Exec, Input, Output},
    shell::Script,
};
use futures::StreamExt as _;
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
    io::{AsyncRead, AsyncReadExt as _, AsyncWrite, AsyncWriteExt as _, DuplexStream},
    process::{Child, Command},
    sync::watch,
};

/// `CREATE_NO_WINDOW` from the Win32 process creation flags.
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// The shell a [`Remote`] process is started through.
const SH: &str = "/bin/sh";

/// The bytes each of a [`Remote`] process's streams buffers on this side.
const PIPE: usize = 64 << 10;

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
    fn spawn<'a>(&'a self, spec: &'a Spec<'a>) -> Spawning<'a>;
}

/// What [`Spawner::spawn`] answers.
pub type Spawning<'a> = Pin<Box<dyn Future<Output = io::Result<Process>> + Send + 'a>>;

/// The spawner for a project, by its root.
pub fn open(_root: &Path) -> Box<dyn Spawner> {
    Box::new(Local::new())
}

/// This machine. Its `PATH` is this process's as [`super::path::adopt`]
/// leaves it.
pub struct Local {
    path: OsString,
}

/// Another machine, through a cydonia-server's [`Exec`]. A process started
/// here inherits the remote user's login shell environment, read once by
/// [`Remote::connect`], and is run by `/bin/sh -c`, so a bare name is looked
/// up on that environment's `PATH`.
pub struct Remote {
    exec: cupola::Handle<Exec>,
    env: Environment,
}

/// A [`Remote`] process, killed when dropped before it has ended.
struct Running {
    input: Input,
    ended: watch::Receiver<bool>,
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

    fn start(&self, spec: &Spec) -> io::Result<Process> {
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

impl Default for Local {
    fn default() -> Self {
        Self::new()
    }
}

impl Spawner for Local {
    fn var(&self, key: &str) -> Option<String> {
        std::env::var(key).ok()
    }

    fn spawn<'a>(&'a self, spec: &'a Spec<'a>) -> Spawning<'a> {
        Box::pin(std::future::ready(self.start(spec)))
    }
}

impl Handle for Child {
    fn ended(&mut self) -> Pin<Box<dyn Future<Output = ()> + Send + '_>> {
        Box::pin(async move {
            let _ = self.wait().await;
        })
    }
}

impl Remote {
    /// Reads the environment `$SHELL -l -i` ends up with on the remote, or
    /// the server's own where that shell has not answered within [`TIMEOUT`].
    pub async fn connect(exec: cupola::Handle<Exec>) -> io::Result<Self> {
        let mut login = Script::new();
        login.raw(r#"exec "${SHELL:-/bin/sh}""#).words(LOGIN);
        let asked = tokio::time::timeout(TIMEOUT, Self::output(&exec, Self::sh(&login))).await;
        let dump = match asked {
            Ok(Ok(dump)) => dump,
            _ => {
                let mut dump = Script::new();
                dump.raw(DUMP);
                Self::output(&exec, Self::sh(&dump)).await?
            }
        };
        Ok(Self {
            exec,
            env: Environment::parse(&dump),
        })
    }

    /// `script`, run by [`SH`].
    fn sh(script: &Script) -> exec::Command {
        let mut command = exec::Command::new(SH);
        command.args = vec!["-c".into(), script.to_os_string()];
        command
    }

    /// What `command` writes to its stdout, run with its stdin closed. Killed
    /// when the future is dropped before the process ends.
    async fn output(exec: &cupola::Handle<Exec>, command: exec::Command) -> io::Result<String> {
        let mut process = exec.start(&command).await.map_err(io::Error::other)?;
        let (_running, ended) = Running::new(process.input());
        process.close_stdin().await.map_err(io::Error::other)?;
        let mut stdout = Vec::new();
        while let Some(output) = process.next().await {
            match output.map_err(io::Error::other)? {
                Output::Stdout(bytes) => stdout.extend(bytes),
                Output::Stderr(_) => {}
                Output::Exit(_) => break,
            }
        }
        let _ = ended.send(true);
        Ok(String::from_utf8_lossy(&stdout).into_owned())
    }

    /// Carries `process`'s streams to pipes on this side, on
    /// [`super::acp::runtime`].
    fn piped(process: exec::Process) -> Process {
        let input = process.input();
        let (stdin, feed) = tokio::io::duplex(PIPE);
        let (stdout, out) = tokio::io::duplex(PIPE);
        let (stderr, err) = tokio::io::duplex(PIPE);
        let (running, ended) = Running::new(input.clone());
        let runtime = super::acp::runtime();
        runtime.spawn(Self::feed(feed, input));
        runtime.spawn(async move {
            Self::drain(process, out, err).await;
            let _ = ended.send(true);
        });
        Process {
            stdin: Box::new(stdin),
            stdout: Box::new(stdout),
            stderr: Box::new(stderr),
            handle: Box::new(running),
        }
    }

    /// Writes what arrives on `feed` to the process's stdin, and closes it
    /// when `feed` ends.
    async fn feed(mut feed: DuplexStream, input: Input) {
        let mut buffer = vec![0; PIPE];
        loop {
            match feed.read(&mut buffer).await {
                Ok(0) | Err(_) => {
                    let _ = input.close_stdin().await;
                    return;
                }
                Ok(n) => {
                    if input.write(&buffer[..n]).await.is_err() {
                        return;
                    }
                }
            }
        }
    }

    /// Writes the process's output to `out` and `err` until it exits or its
    /// server is started afresh, then ends both.
    async fn drain(mut process: exec::Process, mut out: DuplexStream, mut err: DuplexStream) {
        while let Some(Ok(output)) = process.next().await {
            match output {
                Output::Stdout(bytes) => {
                    let _ = out.write_all(&bytes).await;
                }
                Output::Stderr(bytes) => {
                    let _ = err.write_all(&bytes).await;
                }
                Output::Exit(_) => break,
            }
        }
        let _ = out.shutdown().await;
        let _ = err.shutdown().await;
    }
}

impl Spawner for Remote {
    fn var(&self, key: &str) -> Option<String> {
        self.env.get(key).map(str::to_owned)
    }

    fn spawn<'a>(&'a self, spec: &'a Spec<'a>) -> Spawning<'a> {
        Box::pin(async move {
            let mut script = Script::new();
            script.raw("exec").word(spec.program).words(spec.args);
            let mut command = Self::sh(&script);
            command.cwd = Some(spec.cwd.to_owned());
            let set = spec
                .env
                .iter()
                .map(|(key, value)| (key.as_str(), value.as_str()));
            command.env = self
                .env
                .iter()
                .chain(set)
                .map(|(key, value)| (key.into(), value.into()))
                .collect();
            let process = self.exec.start(&command).await.map_err(io::Error::other)?;
            Ok(Self::piped(process))
        })
    }
}

impl Running {
    /// The handle, and the sender that marks the process ended.
    fn new(input: Input) -> (Self, watch::Sender<bool>) {
        let (ended, receiver) = watch::channel(false);
        let running = Self {
            input,
            ended: receiver,
        };
        (running, ended)
    }
}

impl Handle for Running {
    fn ended(&mut self) -> Pin<Box<dyn Future<Output = ()> + Send + '_>> {
        Box::pin(async move {
            let _ = self.ended.wait_for(|ended| *ended).await;
        })
    }
}

impl Drop for Running {
    fn drop(&mut self) {
        if *self.ended.borrow() {
            return;
        }
        let input = self.input.clone();
        super::acp::runtime().spawn(async move {
            let _ = input.kill().await;
        });
    }
}
