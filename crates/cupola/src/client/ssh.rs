//! A [`Transport`] over the system `ssh`: one ControlMaster connection per
//! host, which every other `ssh`, `sftp` and `scp` run here goes through, so
//! the user's `~/.ssh/config`, agent and known hosts apply.
//!
//! Commands run on the remote are read by the remote user's login shell,
//! which must parse POSIX shell syntax. Relative remote paths are relative to
//! the remote home directory.

use std::{
    ffi::OsString,
    fmt, io,
    os::unix::ffi::OsStrExt as _,
    path::{Path, PathBuf},
    pin::pin,
    process::{Command, Stdio},
    sync::{Arc, Mutex},
};

use anyhow::{Context as _, bail};
use futures::{
    AsyncReadExt as _, AsyncWriteExt as _, FutureExt as _,
    future::{BoxFuture, Either, select},
};
use serde::{Deserialize, Serialize};
use tempfile::TempDir;

use super::{
    askpass::Askpass,
    platform::Platform,
    session::{Exit, Link, Mode, Transport},
};
use crate::{Client, Connection, exec, proxy::NO_DAEMON, shell::Script};

const CONTROL_SOCKET: &str = "ssh.sock";
const ASKPASS_SOCKET: &str = "askpass.sock";
/// Where server binaries are installed, relative to the remote home.
const SERVER_DIR: &str = ".cupola";
const SERVER_MODE: &str = "755";

/// A host as `ssh` is given it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "Endpoint")]
pub struct Host {
    destination: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    port: Option<u16>,
}

/// A [`Host`]'s fields as read, before [`Host::new`] checks them.
#[derive(Deserialize)]
struct Endpoint {
    destination: String,
    port: Option<u16>,
}

/// What the embedder supplies for its server on a remote host.
pub trait Delegate: Send + Sync + 'static {
    /// The server binary's file name on the remote, before its version.
    fn name(&self) -> &str;

    /// What the server binary prints, trimmed, for
    /// [`Delegate::version_args`]. An installed binary that prints anything
    /// else is replaced.
    fn version(&self) -> &str;

    fn version_args(&self) -> Vec<OsString>;

    /// The arguments that run the server binary as a
    /// [`proxy`](crate::proxy) reaching its daemon in `mode`.
    fn proxy_args(&self, mode: Mode) -> Vec<OsString>;

    /// A local server binary built for `platform`.
    fn binary(&self, platform: Platform) -> BoxFuture<'static, anyhow::Result<PathBuf>>;

    /// Answers one of ssh's password or 2FA prompts; `None` cancels the
    /// connection.
    fn ask(&self, prompt: String) -> BoxFuture<'static, Option<String>>;

    /// The program ssh runs as its askpass helper. Its `main` must hand the
    /// process to [`Helper::from_env`](super::askpass::Helper::from_env)
    /// before anything else.
    fn askpass_program(&self) -> io::Result<PathBuf> {
        std::env::current_exe()
    }

    /// Registers on `client` the services it answers the server with. Called
    /// on every new client before it is first attached.
    fn serve(&self, client: &Client) {
        let _ = client;
    }
}

/// Where a server binary of one name and version is installed, relative to
/// the remote home: `.cupola/<name>-<version>`, with every character besides
/// ASCII alphanumerics and `._+-` replaced by `_`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ServerPath(PathBuf);

/// The commands that go through one host's ControlMaster socket.
#[derive(Clone, Debug)]
pub struct Control {
    host: Host,
    socket: PathBuf,
}

/// One host's ControlMaster connection, opened on the first
/// [`Transport::open`] and opened again by a later one when it has exited.
/// The first open that connects also installs the server binary; every open
/// starts a proxy.
pub struct Remote<D> {
    control: Control,
    delegate: D,
    server: ServerPath,
    /// Holds the control and askpass sockets.
    _dir: TempDir,
    master: Mutex<Option<async_process::Child>>,
    /// Set once the server binary is installed.
    platform: Mutex<Option<Platform>>,
}

/// Whether a command gets a terminal on the remote.
#[derive(Clone, Copy)]
enum Tty {
    Yes,
    No,
}

impl Host {
    /// `destination` is `[user@]host` or a `Host` alias from the ssh config.
    pub fn new(destination: impl Into<String>) -> anyhow::Result<Self> {
        let destination = destination.into();
        if destination.is_empty() || destination.starts_with('-') {
            bail!("{destination:?} is not an ssh destination");
        }
        Ok(Self {
            destination,
            port: None,
        })
    }

    pub fn port(mut self, port: u16) -> Self {
        self.port = Some(port);
        self
    }

    pub fn destination(&self) -> &str {
        &self.destination
    }
}

impl TryFrom<Endpoint> for Host {
    type Error = anyhow::Error;

    fn try_from(endpoint: Endpoint) -> anyhow::Result<Self> {
        let host = Self::new(endpoint.destination)?;
        Ok(match endpoint.port {
            Some(port) => host.port(port),
            None => host,
        })
    }
}

impl fmt::Display for Host {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.port {
            Some(port) => write!(f, "{}:{port}", self.destination),
            None => f.write_str(&self.destination),
        }
    }
}

impl<D: Delegate> Delegate for Arc<D> {
    fn name(&self) -> &str {
        (**self).name()
    }

    fn version(&self) -> &str {
        (**self).version()
    }

    fn version_args(&self) -> Vec<OsString> {
        (**self).version_args()
    }

    fn proxy_args(&self, mode: Mode) -> Vec<OsString> {
        (**self).proxy_args(mode)
    }

    fn binary(&self, platform: Platform) -> BoxFuture<'static, anyhow::Result<PathBuf>> {
        (**self).binary(platform)
    }

    fn ask(&self, prompt: String) -> BoxFuture<'static, Option<String>> {
        (**self).ask(prompt)
    }

    fn askpass_program(&self) -> io::Result<PathBuf> {
        (**self).askpass_program()
    }

    fn serve(&self, client: &Client) {
        (**self).serve(client)
    }
}

impl ServerPath {
    pub fn new(name: &str, version: &str) -> Self {
        let file: String = format!("{name}-{version}")
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || "._+-".contains(c) {
                    c
                } else {
                    '_'
                }
            })
            .collect();
        Self(Path::new(SERVER_DIR).join(file))
    }

    pub fn as_path(&self) -> &Path {
        &self.0
    }

    /// Where a binary is uploaded before it is moved to this path.
    fn upload(&self) -> PathBuf {
        let mut file = self.0.clone().into_os_string();
        file.push(format!(".upload-{}", std::process::id()));
        file.into()
    }
}

impl Control {
    /// `socket` is the ControlPath the master listens at.
    pub fn new(host: Host, socket: impl Into<PathBuf>) -> Self {
        Self {
            host,
            socket: socket.into(),
        }
    }

    /// The ControlMaster: authenticates, then holds the connection open
    /// without running a command. Its stdout closes once it has connected or
    /// exited.
    pub fn master(&self) -> Command {
        let mut command = Command::new("ssh");
        command
            .args(["-N", "-o", "ControlPersist=no", "-o", "ControlMaster=yes"])
            .arg("-o")
            .arg(self.control_path());
        self.port(&mut command, "-p");
        command.arg(&self.host.destination);
        command
    }

    /// Exits 0 while the master is connected.
    pub fn check(&self) -> Command {
        let mut command = Command::new("ssh");
        command.arg("-o").arg(self.control_path());
        self.port(&mut command, "-p");
        command.args(["-O", "check"]).arg(&self.host.destination);
        command
    }

    /// `command` run on the remote with a terminal, at its directory, or the
    /// remote home when it has none.
    pub fn build_command(&self, command: &exec::Command) -> Command {
        let mut script = Self::at(command.cwd.as_deref());
        script.raw("&& exec env");
        for (key, value) in &command.env {
            let mut pair = key.clone();
            pair.push("=");
            pair.push(value);
            script.word(pair);
        }
        script.word(&command.program).words(&command.args);
        self.ssh(Tty::Yes, &script)
    }

    /// The remote user's login shell with a terminal, at `dir`.
    pub fn login_shell(&self, dir: &Path) -> Command {
        let mut script = Self::at(Some(dir));
        script.raw("&& exec \"$SHELL\" -l");
        self.ssh(Tty::Yes, &script)
    }

    /// The server binary at `server` run as a proxy with `args`, with the
    /// remote home as its directory, on stdio without a terminal.
    pub fn proxy(&self, server: &ServerPath, args: &[OsString]) -> Command {
        let mut script = Self::at(None);
        script.raw("&& exec").word(server.as_path()).words(args);
        self.ssh(Tty::No, &script)
    }

    /// `script` run by the remote shell from the remote home, without a
    /// terminal.
    pub fn run(&self, script: &Script) -> Command {
        let mut at_home = Self::at(None);
        at_home.raw("&&").append(script);
        self.ssh(Tty::No, &at_home)
    }

    /// `sftp` in batch mode, reading its commands from stdin.
    pub fn sftp(&self) -> Command {
        let mut command = Command::new("sftp");
        self.shared(&mut command, "-P");
        command.args(["-b", "-"]).arg(&self.host.destination);
        command
    }

    /// `scp` copying the local `source` to `target` on the remote.
    pub fn scp(&self, source: &Path, target: &Path) -> Command {
        let mut command = Command::new("scp");
        self.shared(&mut command, "-P");
        let mut remote = OsString::from(&self.host.destination);
        remote.push(":");
        remote.push(target);
        command.arg(source).arg(remote);
        command
    }

    fn ssh(&self, tty: Tty, script: &Script) -> Command {
        let mut command = Command::new("ssh");
        self.shared(&mut command, "-p");
        match tty {
            Tty::Yes => command.args(["-o", "LogLevel=ERROR", "-t"]),
            Tty::No => command.arg("-T"),
        };
        command
            .arg(&self.host.destination)
            .arg(script.to_os_string());
        command
    }

    /// The options of a command through the master socket.
    fn shared(&self, command: &mut Command, port_flag: &str) {
        command
            .args(["-o", "ControlMaster=no", "-o"])
            .arg(self.control_path());
        self.port(command, port_flag);
    }

    fn port(&self, command: &mut Command, flag: &str) {
        if let Some(port) = self.host.port {
            command.arg(flag).arg(port.to_string());
        }
    }

    fn control_path(&self) -> OsString {
        let mut option = OsString::from("ControlPath=");
        option.push(&self.socket);
        option
    }

    /// A script that changes to the remote home, then to `dir`.
    fn at(dir: Option<&Path>) -> Script {
        let mut script = Script::new();
        script.raw("cd");
        if let Some(dir) = dir {
            script.raw("&& cd").word(dir);
        }
        script
    }
}

impl<D: Delegate> Remote<D> {
    pub fn new(host: Host, delegate: D) -> io::Result<Self> {
        let dir = tempfile::Builder::new().prefix("cupola-ssh").tempdir()?;
        let server = ServerPath::new(delegate.name(), delegate.version());
        Ok(Self {
            control: Control::new(host, dir.path().join(CONTROL_SOCKET)),
            delegate,
            server,
            _dir: dir,
            master: Mutex::new(None),
            platform: Mutex::new(None),
        })
    }

    /// The commands through this host's master, which reach the remote only
    /// while it is connected.
    pub fn control(&self) -> &Control {
        &self.control
    }

    /// Known once a server binary has been installed.
    pub fn platform(&self) -> Option<Platform> {
        *self.platform.lock().unwrap()
    }

    /// Starts the master unless it is running, answering its prompts with
    /// [`Delegate::ask`], and waits until it is connected.
    async fn connect(&self) -> anyhow::Result<()> {
        {
            let mut master = self.master.lock().unwrap();
            if let Some(child) = master.as_mut()
                && child.try_status()?.is_none()
            {
                return Ok(());
            }
            *master = None;
        }
        let askpass = Askpass::bind(self.control.socket.with_file_name(ASKPASS_SOCKET))?;
        let program = self.delegate.askpass_program()?;
        let mut child = async_process::Command::from(self.control.master())
            .envs(askpass.env(&program))
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .context("start ssh")?;
        let mut stdout = child.stdout.take().context("ssh has no stdout")?;
        let connected = async move {
            let mut ignored = Vec::new();
            stdout.read_to_end(&mut ignored).await
        };
        let prompts = askpass.serve(|prompt| self.delegate.ask(prompt));
        match select(pin!(prompts), pin!(connected)).await {
            Either::Left((Err(error), _)) => {
                return Err(error.context(format!("connect to {}", self.control.host)));
            }
            Either::Left((Ok(never), _)) => match never {},
            Either::Right((read, _)) => {
                read?;
            }
        }
        let checked = async_process::Command::from(self.control.check())
            .stdin(Stdio::null())
            .output()
            .await
            .context("start ssh")?;
        if !checked.status.success() {
            let _ = child.kill();
            let _ = child.status().await;
            let mut stderr = String::new();
            if let Some(mut pipe) = child.stderr.take() {
                pipe.read_to_string(&mut stderr).await?;
            }
            bail!("connect to {}: {}", self.control.host, stderr.trim());
        }
        *self.master.lock().unwrap() = Some(child);
        Ok(())
    }

    /// Detects the remote platform and installs the server binary unless the
    /// one at its path prints [`Delegate::version`].
    async fn install(&self) -> anyhow::Result<Platform> {
        let mut uname = Script::new();
        uname.raw("uname -sm");
        let platform = Platform::from_uname(&self.output(&uname).await?)?;

        let mut version = Script::new();
        version
            .word(self.server.as_path())
            .words(self.delegate.version_args());
        if let Ok(printed) = self.output(&version).await
            && printed.trim() == self.delegate.version()
        {
            return Ok(platform);
        }

        let binary = self.delegate.binary(platform).await?;
        let binary =
            std::path::absolute(&binary).with_context(|| format!("locate {}", binary.display()))?;
        let upload = self.server.upload();
        let mut mkdir = Script::new();
        mkdir.raw("mkdir -p").word(SERVER_DIR);
        self.output(&mkdir).await?;
        self.upload(&binary, &upload).await?;
        let mut place = Script::new();
        place
            .raw("chmod")
            .word(SERVER_MODE)
            .word(&upload)
            .raw("&& mv -f")
            .word(&upload)
            .word(self.server.as_path());
        self.output(&place).await?;
        Ok(platform)
    }

    /// Copies `source` to `target` with sftp, or with scp when sftp fails.
    async fn upload(&self, source: &Path, target: &Path) -> anyhow::Result<()> {
        let mut batch = b"put ".to_vec();
        Self::sftp_quote(source, &mut batch);
        batch.push(b' ');
        Self::sftp_quote(target, &mut batch);
        batch.push(b'\n');
        let sftp = async {
            let mut child = async_process::Command::from(self.control.sftp())
                .stdin(Stdio::piped())
                .stdout(Stdio::null())
                .stderr(Stdio::piped())
                .kill_on_drop(true)
                .spawn()?;
            let mut stdin = child.stdin.take().context("sftp has no stdin")?;
            stdin.write_all(&batch).await?;
            stdin.close().await?;
            drop(stdin);
            anyhow::Ok(child.output().await?)
        };
        if let Ok(output) = sftp.await
            && output.status.success()
        {
            return Ok(());
        }
        let output = async_process::Command::from(self.control.scp(source, target))
            .stdin(Stdio::null())
            .output()
            .await
            .context("start scp")?;
        if !output.status.success() {
            bail!(
                "upload {} to {}: {}",
                source.display(),
                self.control.host,
                String::from_utf8_lossy(&output.stderr).trim()
            );
        }
        Ok(())
    }

    /// What `script` prints, failing unless it exits 0.
    async fn output(&self, script: &Script) -> anyhow::Result<String> {
        let output = async_process::Command::from(self.control.run(script))
            .stdin(Stdio::null())
            .output()
            .await
            .context("start ssh")?;
        if !output.status.success() {
            bail!(
                "{} on {}: {}",
                script.to_string_lossy(),
                self.control.host,
                String::from_utf8_lossy(&output.stderr).trim()
            );
        }
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    }

    /// `path` as one double-quoted sftp batch argument.
    fn sftp_quote(path: &Path, batch: &mut Vec<u8>) {
        batch.push(b'"');
        for &byte in path.as_os_str().as_bytes() {
            if byte == b'"' || byte == b'\\' {
                batch.push(b'\\');
            }
            batch.push(byte);
        }
        batch.push(b'"');
    }

    async fn open_link(&self, mode: Mode) -> anyhow::Result<Link> {
        self.connect().await?;
        if self.platform().is_none() {
            let platform = self.install().await?;
            *self.platform.lock().unwrap() = Some(platform);
        }
        let args = self.delegate.proxy_args(mode);
        let mut child = async_process::Command::from(self.control.proxy(&self.server, &args))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .context("start the proxy")?;
        let stdin = child.stdin.take().context("the proxy has no stdin")?;
        let stdout = child.stdout.take().context("the proxy has no stdout")?;
        let exit = async move {
            match child.status().await {
                Ok(status) if status.code() == Some(NO_DAEMON) => Exit::NoDaemon,
                _ => Exit::Other,
            }
        };
        Ok(Link {
            connection: Connection::new(stdout, stdin),
            exit: exit.boxed(),
        })
    }
}

impl<D: Delegate> Transport for Remote<D> {
    fn open(&self, mode: Mode) -> BoxFuture<'_, anyhow::Result<Link>> {
        self.open_link(mode).boxed()
    }
}
