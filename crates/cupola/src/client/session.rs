//! A [`Client`] kept attached: its transport opened, the connection watched
//! by a heartbeat, and opened again when it ends or falls silent.

use std::{
    convert::Infallible,
    pin::{Pin, pin},
    sync::{
        Mutex,
        atomic::{AtomicBool, Ordering},
    },
    task::{Context, Poll},
    time::Duration,
};

use anyhow::anyhow;
use async_io::Timer;
use futures::{
    Stream, StreamExt as _,
    channel::mpsc::{self, UnboundedReceiver, UnboundedSender},
    future::{BoxFuture, Either, select},
};

use crate::{Client, Connection, ping::Ping};

/// Opens tried in a row, after a connection is lost, before the session
/// gives up. A connection that answers a heartbeat starts the count again.
const ATTEMPTS: usize = 3;

/// Where a session's connection stands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    /// Opening the first connection, until its first heartbeat is answered.
    Connecting,
    /// The last heartbeat was answered.
    Connected,
    /// A connection was lost; opening another, until its first heartbeat is
    /// answered.
    Reconnecting,
    /// Not running: before [`Session::run`], and after it ends or is dropped.
    Disconnected,
}

/// How a transport reaches the server.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    /// Through a server started afresh, replacing one that is running.
    Start,
    /// Through the server already running, whose state the client's
    /// unacknowledged envelopes continue.
    Reconnect,
}

/// How a link ended, as its transport tells it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Exit {
    /// A [`Mode::Reconnect`] found no server running.
    NoDaemon,
    Other,
}

/// One opened connection to the server.
pub struct Link {
    pub connection: Connection,
    /// Resolves when the link has ended. Dropping it closes the link.
    pub exit: BoxFuture<'static, Exit>,
}

/// What a session opens its connections through.
pub trait Transport: Send + Sync + 'static {
    fn open(&self, mode: Mode) -> BoxFuture<'_, anyhow::Result<Link>>;
}

/// How often a session pings, and how many unanswered pings in a row end a
/// connection.
#[derive(Clone, Copy, Debug)]
pub struct Heartbeat {
    pub interval: Duration,
    /// How long one ping waits for its answer.
    pub timeout: Duration,
    pub missed: usize,
}

/// One client kept attached through one transport.
pub struct Session<T> {
    client: Client,
    transport: T,
    heartbeat: Heartbeat,
    watch: Mutex<Watch>,
}

/// A session's status from the time [`Session::statuses`] was called: the
/// status then, and every change after.
pub struct Statuses {
    receiver: UnboundedReceiver<Status>,
}

struct Watch {
    status: Status,
    subscribers: Vec<UnboundedSender<Status>>,
}

/// How one link's attachment ended.
struct Served {
    /// A heartbeat was answered over the link.
    answered: bool,
    /// `None` when the link was closed for missed heartbeats, or ended
    /// without telling how within a heartbeat timeout.
    exit: Option<Exit>,
}

/// Marks the session disconnected when [`Session::run`] ends or is dropped.
struct Stopped<'a, T>(&'a Session<T>);

impl Default for Heartbeat {
    fn default() -> Self {
        Self {
            interval: Duration::from_secs(5),
            timeout: Duration::from_secs(5),
            missed: 5,
        }
    }
}

impl<T: Transport> Session<T> {
    /// The services `client` answers the server with must be registered on
    /// it before [`Session::run`]. Its runner is the caller's to poll.
    pub fn new(client: Client, transport: T) -> Self {
        Self::with_heartbeat(client, transport, Heartbeat::default())
    }

    /// [`Session::new`], pinging as `heartbeat` says.
    pub fn with_heartbeat(client: Client, transport: T, heartbeat: Heartbeat) -> Self {
        Self {
            client,
            transport,
            heartbeat,
            watch: Mutex::new(Watch {
                status: Status::Disconnected,
                subscribers: Vec::new(),
            }),
        }
    }

    /// The client requests go through. Requests made while no connection is
    /// attached go out once one is.
    pub fn client(&self) -> &Client {
        &self.client
    }

    pub fn transport(&self) -> &T {
        &self.transport
    }

    pub fn status(&self) -> Status {
        self.watch.lock().unwrap().status
    }

    pub fn statuses(&self) -> Statuses {
        let (sender, receiver) = mpsc::unbounded();
        let mut watch = self.watch.lock().unwrap();
        let _ = sender.unbounded_send(watch.status);
        watch.subscribers.push(sender);
        Statuses { receiver }
    }

    /// Opens a connection with [`Mode::Start`] and keeps the client attached
    /// until opening fails three times in a row, answering the last error.
    /// Fails at once when the first open fails.
    ///
    /// A lost connection is opened again with [`Mode::Reconnect`], and the
    /// client's unacknowledged envelopes are resent over it. When that link
    /// exits with [`Exit::NoDaemon`], the next is opened with [`Mode::Start`]
    /// and the client's waiting requests fail with
    /// [`Error::Restarted`](crate::Error::Restarted), which every
    /// [`Events`](crate::Events) yields too.
    pub async fn run(&self) -> anyhow::Result<Infallible> {
        let _stopped = Stopped(self);
        self.set(Status::Connecting);
        let mut link = self.transport.open(Mode::Start).await?;
        let mut attempts = 0;
        loop {
            let served = self.serve(link).await;
            if served.answered {
                attempts = 0;
            }
            self.set(Status::Reconnecting);
            let mode = match served.exit {
                Some(Exit::NoDaemon) => Mode::Start,
                Some(Exit::Other) | None => Mode::Reconnect,
            };
            let mut failure = None;
            link = loop {
                if attempts == ATTEMPTS {
                    return Err(failure.unwrap_or_else(|| anyhow!("the connection was lost")));
                }
                attempts += 1;
                match self.transport.open(mode).await {
                    Ok(link) => break link,
                    Err(error) => failure = Some(error),
                }
            };
            if mode == Mode::Start {
                self.client.reset();
            }
        }
    }

    /// Attaches the client over `link` until the connection ends, the link
    /// exits, or the heartbeat is missed [`Heartbeat::missed`] times in a
    /// row.
    async fn serve(&self, link: Link) -> Served {
        let Link {
            connection,
            mut exit,
        } = link;
        let answered = AtomicBool::new(false);
        let attached = pin!(self.client.attach(connection));
        let beating = pin!(self.beat(&answered));
        let told = match select(select(attached, beating), &mut exit).await {
            Either::Left((Either::Left(_), _)) => None,
            Either::Left((Either::Right(_), _)) => Some(None),
            Either::Right((exit, _)) => Some(Some(exit)),
        };
        let exit = match told {
            Some(told) => told,
            None => self.within(&mut exit).await,
        };
        Served {
            answered: answered.load(Ordering::SeqCst),
            exit,
        }
    }

    /// Pings until [`Heartbeat::missed`] pings in a row go unanswered,
    /// marking the session connected on every answer.
    async fn beat(&self, answered: &AtomicBool) {
        let ping = self.client.service::<Ping>();
        let mut missed = 0;
        loop {
            match self.within(ping.request(&())).await {
                Some(Ok(())) => {
                    missed = 0;
                    answered.store(true, Ordering::SeqCst);
                    self.set(Status::Connected);
                }
                Some(Err(_)) | None => {
                    missed += 1;
                    if missed >= self.heartbeat.missed {
                        return;
                    }
                }
            }
            Timer::after(self.heartbeat.interval).await;
        }
    }

    /// `future`'s output, unless [`Heartbeat::timeout`] passes first.
    async fn within<F: Future>(&self, future: F) -> Option<F::Output> {
        let timer = Timer::after(self.heartbeat.timeout);
        match select(pin!(future), timer).await {
            Either::Left((output, _)) => Some(output),
            Either::Right(_) => None,
        }
    }

    fn set(&self, status: Status) {
        self.watch.lock().unwrap().set(status);
    }
}

impl Watch {
    fn set(&mut self, status: Status) {
        if self.status == status {
            return;
        }
        self.status = status;
        self.subscribers
            .retain(|sender| sender.unbounded_send(status).is_ok());
    }
}

impl<T> Drop for Stopped<'_, T> {
    fn drop(&mut self) {
        self.0.watch.lock().unwrap().set(Status::Disconnected);
    }
}

impl Stream for Statuses {
    type Item = Status;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Status>> {
        self.receiver.poll_next_unpin(cx)
    }
}
