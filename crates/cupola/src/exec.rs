//! The `exec` service: programs run on the server's machine, several at once,
//! each named by the id its start was answered with.

use std::{
    collections::HashMap,
    ffi::OsString,
    path::PathBuf,
    pin::Pin,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
    task::{Context, Poll},
};

use anyhow::{Context as _, anyhow};
use async_process::{Child, ChildStdin, Stdio};
use futures::{AsyncRead, AsyncReadExt as _, AsyncWriteExt as _, Stream, StreamExt as _, lock};
use serde::{Deserialize, Serialize};

use crate::{Emitter, Error, Events, Handle, Server, Service, Spawner, endpoint::Epoch};

const CHUNK: usize = 64 << 10;

/// Processes on the server's machine. Every process's events go to every
/// subscriber; [`Handle::start`] gives one process's alone.
pub struct Exec;

/// What to run.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Command {
    /// Looked up on the server's `PATH` when it has no separator.
    #[serde(with = "crate::bytes::os_string")]
    pub program: OsString,
    #[serde(with = "crate::bytes::os_strings")]
    pub args: Vec<OsString>,
    /// The server process's working directory when `None`.
    #[serde(with = "crate::bytes::optional_path")]
    pub cwd: Option<PathBuf>,
    /// Set over the server process's environment.
    #[serde(with = "crate::bytes::os_pairs")]
    pub env: Vec<(OsString, OsString)>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Request {
    /// Answered with [`Response::Started`].
    Start(Command),
    /// Written to the process's stdin. Answered with [`Response::Done`] once
    /// written.
    Stdin { id: u64, bytes: Vec<u8> },
    /// Answered with [`Response::Done`].
    CloseStdin { id: u64 },
    /// Answered with [`Response::Done`] once the signal is sent; the process's
    /// [`Output::Exit`] follows.
    Kill { id: u64 },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Response {
    /// The id every later request and event about the process carries.
    Started {
        id: u64,
    },
    Done,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Event {
    pub id: u64,
    pub output: Output,
}

/// A process's events, in the order they happened on each stream. Nothing
/// orders stdout against stderr. [`Output::Exit`] is the last event of a
/// process and follows the end of both streams.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Output {
    Stdout(Vec<u8>),
    Stderr(Vec<u8>),
    /// `None` when the process was ended by a signal or its status could not
    /// be read.
    Exit(Option<i32>),
}

/// One started process on the client: its events alone, ending after its
/// [`Output::Exit`], or after [`Error::Restarted`] when the server it ran on
/// is started afresh. Its requests fail with [`Error::Restarted`], unsent,
/// once that server is replaced.
pub struct Process {
    input: Input,
    events: Events<Exec>,
    ended: bool,
}

/// A started process's requests apart from its events, failing as its
/// [`Process`]'s do.
#[derive(Clone)]
pub struct Input {
    id: u64,
    /// The server the process runs on.
    epoch: Epoch,
    handle: Handle<Exec>,
}

/// The server's side: the processes still running.
struct Processes {
    next: AtomicU64,
    running: Mutex<HashMap<u64, Running>>,
    emitter: Emitter<Exec>,
    spawner: Spawner,
}

struct Running {
    child: Child,
    stdin: Arc<lock::Mutex<Option<ChildStdin>>>,
}

impl Service for Exec {
    const NAME: &'static str = "exec";
    type Request = Request;
    type Response = Response;
    type Event = Event;
}

impl Exec {
    /// Answers [`Exec`] requests on `server`. A process's output is read on
    /// the server's [`Runner`](crate::Runner).
    pub fn register(server: &Server) {
        let processes = Arc::new(Processes {
            next: AtomicU64::new(1),
            running: Mutex::default(),
            emitter: server.emitter(),
            spawner: server.spawner(),
        });
        server.register::<Self, _, _>(move |request| {
            let processes = processes.clone();
            async move { processes.answer(request).await }
        });
    }
}

impl Command {
    pub fn new(program: impl Into<OsString>) -> Self {
        Self {
            program: program.into(),
            args: Vec::new(),
            cwd: None,
            env: Vec::new(),
        }
    }
}

impl Processes {
    async fn answer(self: Arc<Self>, request: Request) -> anyhow::Result<Response> {
        match request {
            Request::Start(command) => self.start(command).map(|id| Response::Started { id }),
            Request::Stdin { id, bytes } => {
                let stdin = self.stdin(id)?;
                let mut stdin = stdin.lock().await;
                let stdin = stdin
                    .as_mut()
                    .with_context(|| format!("stdin of process {id} is closed"))?;
                stdin.write_all(&bytes).await?;
                stdin.flush().await?;
                Ok(Response::Done)
            }
            Request::CloseStdin { id } => {
                self.stdin(id)?.lock().await.take();
                Ok(Response::Done)
            }
            Request::Kill { id } => {
                let mut running = self.running.lock().unwrap();
                let process = running
                    .get_mut(&id)
                    .with_context(|| format!("no process {id} is running"))?;
                process.child.kill()?;
                Ok(Response::Done)
            }
        }
    }

    fn start(self: &Arc<Self>, command: Command) -> anyhow::Result<u64> {
        let mut process = async_process::Command::new(&command.program);
        process
            .args(&command.args)
            .envs(command.env)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        if let Some(cwd) = &command.cwd {
            process.current_dir(cwd);
        }
        let mut child = process
            .spawn()
            .with_context(|| format!("start {}", command.program.display()))?;
        let (Some(stdin), Some(stdout), Some(stderr)) =
            (child.stdin.take(), child.stdout.take(), child.stderr.take())
        else {
            return Err(anyhow!("a piped stream is missing"));
        };
        let status = child.status();
        let id = self.next.fetch_add(1, Ordering::Relaxed);
        self.running.lock().unwrap().insert(
            id,
            Running {
                child,
                stdin: Arc::new(lock::Mutex::new(Some(stdin))),
            },
        );
        let processes = self.clone();
        self.spawner.spawn(async move {
            let (_, _, status) = futures::join!(
                processes.forward(id, stdout, Output::Stdout),
                processes.forward(id, stderr, Output::Stderr),
                status,
            );
            processes.running.lock().unwrap().remove(&id);
            let code = status.ok().and_then(|status| status.code());
            let _ = processes.emitter.emit(&Event {
                id,
                output: Output::Exit(code),
            });
        });
        Ok(id)
    }

    fn stdin(&self, id: u64) -> anyhow::Result<Arc<lock::Mutex<Option<ChildStdin>>>> {
        self.running
            .lock()
            .unwrap()
            .get(&id)
            .map(|process| process.stdin.clone())
            .with_context(|| format!("no process {id} is running"))
    }

    /// Emits what `stream` yields until it ends or fails.
    async fn forward(
        &self,
        id: u64,
        mut stream: impl AsyncRead + Unpin,
        output: fn(Vec<u8>) -> Output,
    ) {
        let mut buffer = vec![0; CHUNK];
        while let Ok(n @ 1..) = stream.read(&mut buffer).await {
            let event = Event {
                id,
                output: output(buffer[..n].to_vec()),
            };
            if self.emitter.emit(&event).is_err() {
                return;
            }
        }
    }
}

impl Handle<Exec> {
    /// Starts `command` and answers with its process. Its events are gathered
    /// from before the request is sent.
    pub async fn start(&self, command: &Command) -> Result<Process, Error> {
        let events = self.events();
        match self
            .request_within(&Request::Start(command.clone()), None)
            .await?
        {
            (Response::Started { id }, epoch) => Ok(Process {
                input: Input {
                    id,
                    epoch,
                    handle: self.clone(),
                },
                events,
                ended: false,
            }),
            (Response::Done, _) => Err(Error::Remote {
                service: Exec::NAME.to_owned(),
                message: "a start was answered without an id".to_owned(),
            }),
        }
    }
}

impl Process {
    pub fn id(&self) -> u64 {
        self.input.id
    }

    pub fn input(&self) -> Input {
        self.input.clone()
    }

    pub async fn write(&self, bytes: &[u8]) -> Result<(), Error> {
        self.input.write(bytes).await
    }

    pub async fn close_stdin(&self) -> Result<(), Error> {
        self.input.close_stdin().await
    }

    pub async fn kill(&self) -> Result<(), Error> {
        self.input.kill().await
    }
}

impl Input {
    pub fn id(&self) -> u64 {
        self.id
    }

    pub async fn write(&self, bytes: &[u8]) -> Result<(), Error> {
        self.request(&Request::Stdin {
            id: self.id,
            bytes: bytes.to_vec(),
        })
        .await
    }

    pub async fn close_stdin(&self) -> Result<(), Error> {
        self.request(&Request::CloseStdin { id: self.id }).await
    }

    pub async fn kill(&self) -> Result<(), Error> {
        self.request(&Request::Kill { id: self.id }).await
    }

    async fn request(&self, request: &Request) -> Result<(), Error> {
        self.handle
            .request_within(request, Some(self.epoch))
            .await
            .map(drop)
    }
}

impl Stream for Process {
    type Item = Result<Output, Error>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        while !self.ended {
            match self.events.poll_next_unpin(cx) {
                Poll::Ready(Some(Ok(event))) if event.id == self.input.id => {
                    self.ended = matches!(event.output, Output::Exit(_));
                    return Poll::Ready(Some(Ok(event.output)));
                }
                Poll::Ready(Some(Ok(_))) => {}
                Poll::Ready(Some(Err(error))) => {
                    self.ended = matches!(error, Error::Restarted);
                    return Poll::Ready(Some(Err(error)));
                }
                Poll::Ready(None) => return Poll::Ready(None),
                Poll::Pending => return Poll::Pending,
            }
        }
        Poll::Ready(None)
    }
}
