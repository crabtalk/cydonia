//! The proxy: what ssh runs on the remote, bridging its stdio to a
//! [`Daemon`]'s socket.

use std::{
    io::{self, Read, Write},
    net::Shutdown,
    os::unix::{net::UnixStream, process::CommandExt as _},
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

use crate::daemon::Daemon;

/// The exit code of a proxy that found no daemon to reach.
pub const NO_DAEMON: i32 = 90;

/// How long a proxy waits for a daemon it started to take a connection.
const START_TIMEOUT: Duration = Duration::from_secs(10);
const START_POLL: Duration = Duration::from_millis(20);
const CHUNK: usize = 64 << 10;

/// How a proxy finds its daemon.
pub enum Launch {
    /// The daemon already running, or [`Error::NoDaemon`].
    Reconnect,
    /// A daemon started with this command, after the one running is stopped.
    /// The command must run [`Daemon::run`] on the same directory; it is
    /// started with null stdio in a process group of its own.
    Start(Command),
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("no daemon is running")]
    NoDaemon,
    #[error("the daemon took no connection within {0:?}")]
    Timeout(Duration),
    #[error("i/o: {0}")]
    Io(#[from] io::Error),
}

impl Error {
    /// [`NO_DAEMON`] for [`Error::NoDaemon`], 1 for the rest.
    pub fn exit_code(&self) -> i32 {
        match self {
            Self::NoDaemon => NO_DAEMON,
            Self::Timeout(_) | Self::Io(_) => 1,
        }
    }
}

impl Daemon {
    /// Copies `input` to the daemon's socket and the socket to `output`, until
    /// the socket ends or either side fails. `input` is read on a thread of its
    /// own, which ends the socket's write half when `input` ends.
    ///
    /// Answers [`Error::NoDaemon`] when the daemon is gone at the end.
    pub fn proxy(
        &self,
        launch: Launch,
        input: impl Read + Send + 'static,
        output: impl Write,
    ) -> Result<(), Error> {
        let socket = match launch {
            Launch::Reconnect => {
                if self.running().is_none() {
                    return Err(Error::NoDaemon);
                }
                UnixStream::connect(&self.socket)?
            }
            Launch::Start(command) => self.start(command)?,
        };
        let bridged = Self::bridge(socket, input, output);
        if self.running().is_none() {
            return Err(Error::NoDaemon);
        }
        match bridged {
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::BrokenPipe | io::ErrorKind::ConnectionReset
                ) =>
            {
                Ok(())
            }
            bridged => bridged.map_err(Error::from),
        }
    }

    fn start(&self, mut command: Command) -> Result<UnixStream, Error> {
        self.stop()?;
        let mut daemon = command
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .process_group(0)
            .spawn()?;
        thread::spawn(move || daemon.wait());
        let started = Instant::now();
        loop {
            match UnixStream::connect(&self.socket) {
                Ok(socket) => return Ok(socket),
                Err(_) if started.elapsed() < START_TIMEOUT => thread::sleep(START_POLL),
                Err(_) => return Err(Error::Timeout(START_TIMEOUT)),
            }
        }
    }

    fn bridge(
        mut socket: UnixStream,
        mut input: impl Read + Send + 'static,
        mut output: impl Write,
    ) -> io::Result<()> {
        let mut writer = socket.try_clone()?;
        thread::spawn(move || {
            let _ = io::copy(&mut input, &mut writer);
            let _ = writer.shutdown(Shutdown::Write);
        });
        let mut buffer = vec![0; CHUNK];
        loop {
            let n = match socket.read(&mut buffer) {
                Ok(0) => return Ok(()),
                Ok(n) => n,
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                Err(error) => return Err(error),
            };
            output.write_all(&buffer[..n])?;
            output.flush()?;
        }
    }
}
