//! ssh's password and 2FA prompts, answered by the embedder.
//!
//! ssh runs the embedder's own binary as `SSH_ASKPASS`, with the prompt as
//! its argument and [`SOCKET_VAR`] naming a socket in its environment. That
//! process is a [`Helper`]: it sends the prompt over the socket to the
//! [`Askpass`] that set the environment, and prints the answer for ssh.
//!
//! On the socket, the helper writes the prompt and ends its write half; the
//! answer comes back as a `1` byte and the answer's bytes, or the socket closes
//! with nothing written when the prompt was cancelled.

use std::{
    convert::Infallible,
    ffi::OsString,
    io::{self, Read as _, Write},
    net::Shutdown,
    os::unix::{ffi::OsStringExt as _, net::UnixStream},
    path::{Path, PathBuf},
    process::ExitCode,
};

use anyhow::bail;
use async_net::unix::UnixListener;
use futures::{AsyncReadExt as _, AsyncWriteExt as _, future::BoxFuture};

/// The environment variable naming the socket a [`Helper`] answers through.
pub const SOCKET_VAR: &str = "CUPOLA_ASKPASS";

const ANSWERED: u8 = 1;

/// Where ssh's prompts arrive, while it lives.
pub struct Askpass {
    socket: PathBuf,
    listener: UnixListener,
}

/// The embedder's binary run by ssh as its askpass program.
pub struct Helper {
    socket: PathBuf,
}

impl Askpass {
    /// Listens at `socket`, replacing a file left there.
    pub fn bind(socket: impl Into<PathBuf>) -> io::Result<Self> {
        let socket = socket.into();
        match std::fs::remove_file(&socket) {
            Err(error) if error.kind() != io::ErrorKind::NotFound => return Err(error),
            _ => {}
        }
        let listener = UnixListener::bind(&socket)?;
        Ok(Self { socket, listener })
    }

    /// The environment that makes ssh run `program` as a [`Helper`] for this
    /// askpass, prompting even with a terminal attached.
    pub fn env(&self, program: &Path) -> [(OsString, OsString); 3] {
        [
            ("SSH_ASKPASS".into(), program.into()),
            ("SSH_ASKPASS_REQUIRE".into(), "force".into()),
            (SOCKET_VAR.into(), self.socket.clone().into()),
        ]
    }

    /// Answers prompts one at a time with `ask`, until `ask` answers `None`
    /// or the socket fails.
    pub async fn serve(
        &self,
        ask: impl Fn(String) -> BoxFuture<'static, Option<String>>,
    ) -> anyhow::Result<Infallible> {
        loop {
            let (mut stream, _) = self.listener.accept().await?;
            let mut prompt = Vec::new();
            stream.read_to_end(&mut prompt).await?;
            let Some(answer) = ask(String::from_utf8_lossy(&prompt).into_owned()).await else {
                bail!("the prompt was cancelled");
            };
            stream.write_all(&[ANSWERED]).await?;
            stream.write_all(answer.as_bytes()).await?;
            stream.close().await?;
        }
    }
}

impl Drop for Askpass {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.socket);
    }
}

impl Helper {
    /// The helper this process was started as, when ssh started it as one.
    pub fn from_env() -> Option<Self> {
        std::env::var_os(SOCKET_VAR).map(|socket| Self::at(PathBuf::from(socket)))
    }

    pub fn at(socket: impl Into<PathBuf>) -> Self {
        Self {
            socket: socket.into(),
        }
    }

    /// Asks with this process's arguments, joined by spaces, and prints the
    /// answer to stdout. Fails when there is no answer.
    pub fn main(&self) -> ExitCode {
        let prompt = std::env::args_os()
            .skip(1)
            .map(OsString::into_vec)
            .collect::<Vec<_>>()
            .join(&b' ');
        let mut stdout = io::stdout().lock();
        match self.answer(&prompt, &mut stdout) {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => {
                eprintln!("askpass: {error}");
                ExitCode::FAILURE
            }
        }
    }

    /// Sends `prompt` and writes the answer to `output`. Fails with
    /// [`io::ErrorKind::Interrupted`] when the prompt was cancelled.
    pub fn answer(&self, prompt: &[u8], output: &mut impl Write) -> io::Result<()> {
        let mut stream = UnixStream::connect(&self.socket)?;
        stream.write_all(prompt)?;
        stream.shutdown(Shutdown::Write)?;
        let mut reply = Vec::new();
        stream.read_to_end(&mut reply)?;
        match reply.split_first() {
            Some((&ANSWERED, answer)) => {
                output.write_all(answer)?;
                output.flush()
            }
            _ => Err(io::Error::new(
                io::ErrorKind::Interrupted,
                "the prompt was cancelled",
            )),
        }
    }
}
