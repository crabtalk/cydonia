//! A [`Server`] run as a daemon that outlives its connections, reached through
//! a unix socket in a state directory beside its pid file. Processes on the
//! daemon's machine reach it through a second socket there, each with a
//! server of its own: see [`Daemon::welcome`] and [`Daemon::join`].

use std::{
    fs, io,
    path::{Path, PathBuf},
    process::{self, Command, Stdio},
};

use async_net::unix::{UnixListener, UnixStream};
use futures::{FutureExt as _, StreamExt as _, future, stream::FuturesUnordered};

use crate::{Connection, Server};

const PID_FILE: &str = "cupola.pid";
const SOCKET: &str = "cupola.sock";
const LOCAL_SOCKET: &str = "local.sock";

/// The daemon whose files are in one state directory.
pub struct Daemon {
    pub(crate) pid_file: PathBuf,
    pub(crate) socket: PathBuf,
    local: PathBuf,
}

/// A process id, as written in a pid file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Pid(u32);

impl Daemon {
    pub fn at(dir: impl AsRef<Path>) -> Self {
        let dir = dir.as_ref();
        Self {
            pid_file: dir.join(PID_FILE),
            socket: dir.join(SOCKET),
            local: dir.join(LOCAL_SOCKET),
        }
    }

    /// Serves `server` on the socket until accepting a connection fails.
    ///
    /// Creates the directory, writes this process's pid file, then binds the
    /// socket, replacing a file left at its path. Every connection is attached
    /// as it is accepted; each one attached takes the server's writes from the
    /// ones before it.
    pub async fn run(&self, server: &Server) -> io::Result<()> {
        if let Some(dir) = self.socket.parent() {
            fs::create_dir_all(dir)?;
        }
        fs::write(&self.pid_file, process::id().to_string())?;
        Self::remove(&self.socket)?;
        let listener = UnixListener::bind(&self.socket)?;
        let mut attached = FuturesUnordered::new();
        loop {
            futures::select! {
                accepted = listener.accept().fuse() => {
                    let (stream, _) = accepted?;
                    attached.push(server.attach(Connection::new(stream.clone(), stream)));
                }
                _ = attached.select_next_some() => {}
            }
        }
    }

    /// Serves every process on this machine that [`Daemon::join`]s, until
    /// accepting a connection fails. Each connection gets a [`Server`] of its
    /// own, with the services `serve` registers on it, for as long as it
    /// lasts; a connection is never attached again.
    ///
    /// Creates the directory, then binds the local socket, replacing a file
    /// left at its path.
    pub async fn welcome(&self, serve: impl Fn(&Server)) -> io::Result<()> {
        if let Some(dir) = self.local.parent() {
            fs::create_dir_all(dir)?;
        }
        Self::remove(&self.local)?;
        let listener = UnixListener::bind(&self.local)?;
        let mut joined = FuturesUnordered::new();
        loop {
            futures::select! {
                accepted = listener.accept().fuse() => {
                    let (stream, _) = accepted?;
                    let (server, runner) = Server::new();
                    serve(&server);
                    let attached = async move {
                        let _ = server.attach(Connection::new(stream.clone(), stream)).await;
                    };
                    joined.push(future::join(runner, attached));
                }
                _ = joined.select_next_some() => {}
            }
        }
    }

    /// A connection to the running daemon from a process on its machine, for
    /// a [`Client`](crate::Client) of that process's own, answered by the
    /// server [`Daemon::welcome`] makes for it. Fails with
    /// [`io::ErrorKind::NotFound`] when no daemon is running.
    pub async fn join(&self) -> io::Result<Connection> {
        if self.running().is_none() {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                "no daemon is running",
            ));
        }
        let stream = UnixStream::connect(&self.local).await?;
        Ok(Connection::new(stream.clone(), stream))
    }

    /// Removes the pid file and sockets, then ends the daemon that was
    /// running from this directory, if one was.
    pub fn stop(&self) -> io::Result<()> {
        let running = self.running();
        Self::remove(&self.pid_file)?;
        Self::remove(&self.socket)?;
        Self::remove(&self.local)?;
        match running {
            Some(pid) => pid.terminate(),
            None => Ok(()),
        }
    }

    /// The pid in the pid file, while a process has it.
    pub(crate) fn running(&self) -> Option<Pid> {
        let pid = fs::read_to_string(&self.pid_file)
            .ok()?
            .trim()
            .parse()
            .ok()?;
        Some(Pid(pid)).filter(|pid| pid.alive())
    }

    fn remove(file: &Path) -> io::Result<()> {
        match fs::remove_file(file) {
            Err(error) if error.kind() != io::ErrorKind::NotFound => Err(error),
            _ => Ok(()),
        }
    }
}

impl Pid {
    /// A zombie counts as alive.
    pub(crate) fn alive(self) -> bool {
        Self::kill(["-0", &self.0.to_string()]).is_ok_and(|status| status.success())
    }

    fn terminate(self) -> io::Result<()> {
        Self::kill([self.0.to_string().as_str()]).map(drop)
    }

    fn kill<const N: usize>(args: [&str; N]) -> io::Result<process::ExitStatus> {
        Command::new("kill")
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
    }
}
