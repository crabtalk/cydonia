//! The saved hosts and one [`Session`] per connected host: an embedder gets a
//! host's [`Client`] only from a [`Controller`], and learns each host's
//! [`HostStatus`] from it.

mod hosts;

use std::{
    collections::HashMap,
    path::PathBuf,
    pin::{Pin, pin},
    sync::{Arc, Mutex, Weak},
    task::{Context, Poll},
};

use anyhow::Context as _;
use futures::{
    FutureExt as _, Stream, StreamExt as _,
    channel::mpsc::{self, UnboundedReceiver, UnboundedSender},
    future::{AbortHandle, Abortable, Either, select},
};

use self::hosts::Hosts;
pub use self::hosts::Saved;
use super::{
    host::HostId,
    session::{Heartbeat, Session, Status, Transport},
    ssh::{Delegate, Host, Remote},
};
use crate::{Client, Runner, Spawner};

/// What a controller opens a host's connections through.
pub trait Connector: Send + Sync + 'static {
    type Transport: Transport;

    /// The transport of a new session to `host`.
    fn transport(&self, host: &Host) -> anyhow::Result<Self::Transport>;

    /// Registers on `client` the services it answers the server with. Called
    /// on every new client before its session first opens.
    fn serve(&self, client: &Client) {
        let _ = client;
    }
}

/// Connects over the system `ssh`, one [`Remote`] per session, with one
/// delegate for every host.
pub struct Ssh<D> {
    delegate: Arc<D>,
}

/// Keeps the saved hosts and the session of every connected host. Clones
/// share them.
pub struct Controller<C: Connector> {
    inner: Arc<Inner<C>>,
}

/// A host's [`Status`], and why it is disconnected when its session ended
/// on its own.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HostStatus {
    status: Status,
    /// Set only with [`Status::Disconnected`].
    failure: Option<String>,
}

/// Every host's status change, from the time [`Controller::statuses`] was
/// called. Ends when the controller and every clone of it are dropped.
pub struct Changes {
    receiver: UnboundedReceiver<(HostId, HostStatus)>,
}

/// Locks are taken in field order: `live`, then `hosts`, then `watch`.
struct Inner<C: Connector> {
    live: Mutex<HashMap<HostId, Live<C::Transport>>>,
    hosts: Mutex<Hosts>,
    watch: Mutex<Watch>,
    connector: C,
    heartbeat: Heartbeat,
    spawner: Spawner,
}

/// A connected host's session. Dropping it stops the session and closes its
/// client.
struct Live<T: Transport> {
    session: Arc<Session<T>>,
    abort: AbortHandle,
}

struct Watch {
    /// A host missing here is [`Status::Disconnected`] without a failure.
    statuses: HashMap<HostId, HostStatus>,
    subscribers: Vec<UnboundedSender<(HostId, HostStatus)>>,
}

impl<D: Delegate> Ssh<D> {
    pub fn new(delegate: D) -> Self {
        Self {
            delegate: Arc::new(delegate),
        }
    }
}

impl<D: Delegate> Connector for Ssh<D> {
    type Transport = Remote<Arc<D>>;

    fn transport(&self, host: &Host) -> anyhow::Result<Self::Transport> {
        Remote::new(host.clone(), self.delegate.clone())
            .with_context(|| format!("prepare ssh to {host}"))
    }

    fn serve(&self, client: &Client) {
        self.delegate.serve(client);
    }
}

impl<C: Connector> Controller<C> {
    /// A controller over the hosts saved at `path`, which every change to
    /// them replaces whole. Sessions run only while the runner is polled; it
    /// completes once every clone of the controller is dropped.
    pub fn open(
        path: impl Into<PathBuf>,
        connector: C,
        heartbeat: Heartbeat,
    ) -> anyhow::Result<(Self, Runner)> {
        let hosts = Hosts::load(path.into())?;
        let (spawner, runner) = Runner::new();
        let inner = Inner {
            live: Mutex::default(),
            hosts: Mutex::new(hosts),
            watch: Mutex::new(Watch {
                statuses: HashMap::new(),
                subscribers: Vec::new(),
            }),
            connector,
            heartbeat,
            spawner,
        };
        Ok((
            Self {
                inner: Arc::new(inner),
            },
            runner,
        ))
    }

    /// In the order they were added.
    pub fn hosts(&self) -> Vec<Saved> {
        self.inner.hosts.lock().unwrap().list().to_vec()
    }

    /// Fails for an empty name, and when the file cannot be written.
    pub fn add(&self, name: &str, host: Host) -> anyhow::Result<HostId> {
        self.inner.hosts.lock().unwrap().add(name, host)
    }

    /// Fails for an empty name, an unknown host, and when the file cannot be
    /// written.
    pub fn rename(&self, id: HostId, name: &str) -> anyhow::Result<()> {
        self.inner.hosts.lock().unwrap().rename(id, name)
    }

    /// Disconnects the host, as [`Controller::disconnect`] does, and forgets
    /// it. Fails for an unknown host, and when the file cannot be written;
    /// the host stays as it was then.
    pub fn remove(&self, id: HostId) -> anyhow::Result<()> {
        let mut live = self.inner.live.lock().unwrap();
        self.inner.hosts.lock().unwrap().remove(id)?;
        let gone = live.remove(&id);
        let mut watch = self.inner.watch.lock().unwrap();
        watch.set(id, Status::Disconnected.into());
        watch.statuses.remove(&id);
        drop(watch);
        drop(live);
        drop(gone);
        Ok(())
    }

    /// The client of the host's session, which every caller gets until the
    /// host is disconnected. The first call after that starts a new session
    /// on the [`Runner`]; its requests go out once it has connected, and the
    /// server's requests to it are answered while the session runs.
    pub fn client(&self, id: HostId) -> anyhow::Result<Client> {
        let mut live = self.inner.live.lock().unwrap();
        if let Some(running) = live.get(&id) {
            return Ok(running.session.client().clone());
        }
        let host = self
            .inner
            .hosts
            .lock()
            .unwrap()
            .get(id)
            .with_context(|| format!("no host {id}"))?
            .host()
            .clone();
        let transport = self.inner.connector.transport(&host)?;
        let (client, handlers) = Client::new();
        self.inner.connector.serve(&client);
        let session = Session::with_heartbeat(client.clone(), transport, self.inner.heartbeat);
        let session = Arc::new(session);
        let (abort, registration) = AbortHandle::new_pair();
        let serve = Inner::serve(Arc::downgrade(&self.inner), id, session.clone());
        let job = async move {
            select(pin!(serve), handlers).await;
        };
        self.inner
            .spawner
            .spawn(Abortable::new(job, registration).map(drop));
        live.insert(id, Live { session, abort });
        Ok(client)
    }

    /// Stops the host's session, if it has one, and closes its client: a
    /// request through a clone of it that a caller still holds fails with
    /// [`Error::Closed`](crate::Error::Closed). A session that ends on its own
    /// closes its client the same way.
    pub fn disconnect(&self, id: HostId) {
        let mut live = self.inner.live.lock().unwrap();
        if let Some(gone) = live.remove(&id) {
            self.inner
                .watch
                .lock()
                .unwrap()
                .set(id, Status::Disconnected.into());
            drop(live);
            drop(gone);
        }
    }

    /// What `read` makes of the transport of the host's session, while it has
    /// one. `read` runs with the controller's sessions locked.
    pub fn with_transport<R>(
        &self,
        id: HostId,
        read: impl FnOnce(&C::Transport) -> R,
    ) -> Option<R> {
        let live = self.inner.live.lock().unwrap();
        live.get(&id)
            .map(|running| read(running.session.transport()))
    }

    pub fn status(&self, id: HostId) -> HostStatus {
        self.inner.watch.lock().unwrap().status(id)
    }

    /// Every saved host's status now, in the order they were added, and the
    /// changes after.
    pub fn statuses(&self) -> (Vec<(HostId, HostStatus)>, Changes) {
        let hosts = self.inner.hosts.lock().unwrap();
        let mut watch = self.inner.watch.lock().unwrap();
        let now = hosts
            .list()
            .iter()
            .map(|saved| (saved.id(), watch.status(saved.id())))
            .collect();
        let (sender, receiver) = mpsc::unbounded();
        watch.subscribers.push(sender);
        (now, Changes { receiver })
    }
}

impl<C: Connector> Clone for Controller<C> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}

impl<C: Connector> Inner<C> {
    /// Runs `session`, passing its statuses on while it is the host's, and
    /// disconnects the host with the session's error when it ends.
    async fn serve(inner: Weak<Self>, id: HostId, session: Arc<Session<C::Transport>>) {
        let statuses = session.statuses();
        let forward = statuses.for_each(|status| {
            if let Some(inner) = inner.upgrade() {
                inner.publish(id, &session, status.into());
            }
            futures::future::ready(())
        });
        let failure = match select(pin!(session.run()), pin!(forward)).await {
            Either::Left((Err(error), _)) => Some(format!("{error:#}")),
            Either::Right(_) => None,
        };
        if let Some(inner) = inner.upgrade() {
            inner.ended(id, &session, failure);
        }
    }

    fn publish(&self, id: HostId, session: &Arc<Session<C::Transport>>, status: HostStatus) {
        let live = self.live.lock().unwrap();
        if Self::holds(&live, id, session) {
            self.watch.lock().unwrap().set(id, status);
        }
    }

    fn ended(&self, id: HostId, session: &Arc<Session<C::Transport>>, failure: Option<String>) {
        let mut live = self.live.lock().unwrap();
        if Self::holds(&live, id, session) {
            let gone = live.remove(&id);
            let status = HostStatus {
                status: Status::Disconnected,
                failure,
            };
            self.watch.lock().unwrap().set(id, status);
            drop(live);
            drop(gone);
        }
    }

    /// Whether `session` is the host's live one.
    fn holds(
        live: &HashMap<HostId, Live<C::Transport>>,
        id: HostId,
        session: &Arc<Session<C::Transport>>,
    ) -> bool {
        live.get(&id)
            .is_some_and(|running| Arc::ptr_eq(&running.session, session))
    }
}

impl HostStatus {
    pub fn status(&self) -> Status {
        self.status
    }

    /// The error the host's session ended with, when it ended on its own.
    pub fn failure(&self) -> Option<&str> {
        self.failure.as_deref()
    }
}

impl From<Status> for HostStatus {
    fn from(status: Status) -> Self {
        Self {
            status,
            failure: None,
        }
    }
}

impl<T: Transport> Drop for Live<T> {
    fn drop(&mut self) {
        self.abort.abort();
        self.session.client().close();
    }
}

impl Watch {
    fn status(&self, id: HostId) -> HostStatus {
        self.statuses
            .get(&id)
            .cloned()
            .unwrap_or_else(|| Status::Disconnected.into())
    }

    fn set(&mut self, id: HostId, status: HostStatus) {
        if self.status(id) == status {
            return;
        }
        self.statuses.insert(id, status.clone());
        self.subscribers
            .retain(|sender| sender.unbounded_send((id, status.clone())).is_ok());
    }
}

impl Stream for Changes {
    type Item = (HostId, HostStatus);

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.receiver.poll_next_unpin(cx)
    }
}
