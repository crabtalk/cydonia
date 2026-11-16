//! The hosts the app reaches projects on, and the projects it has opened on
//! them: one [`Remotes`] for the process, installed once over the [`Hosts`]
//! that hand out each host's client. Nothing is reachable on another machine
//! before [`Remotes::install`].
//!
//! Every per-project pick that routes a [`Location::Remote`] asks here: the
//! store, git, the files and the spawner.

#[cfg(unix)]
mod binary;
#[cfg(unix)]
mod ssh;

#[cfg(unix)]
pub use ssh::{Build, Delegate, Prompt, Step};

use crate::agent::{acp, spawner};
use anyhow::Context as _;
use artifact::{location::Location, project::remote};
#[cfg(unix)]
use cupola::client::ssh::Control;
use cupola::{Client, client::host::HostId};
use futures::{
    FutureExt as _,
    future::{BoxFuture, Shared},
};
use std::{
    collections::HashMap,
    io,
    sync::{Arc, Mutex, MutexGuard, OnceLock},
};

static REMOTES: OnceLock<Remotes> = OnceLock::new();

/// What hands out a host's client: Cupola's controller.
pub trait Hosts: Send + Sync + 'static {
    /// The client of the host's session, starting one where it has none.
    fn client(&self, host: HostId) -> anyhow::Result<Client>;

    /// The commands through the host's ssh connection, while it has one.
    #[cfg(unix)]
    fn control(&self, host: HostId) -> Option<Control>;
}

/// The hosts, and what is open on them. Opened projects are kept until
/// [`Remotes::close`]; their replicas are kept fresh on [`acp::runtime`].
pub struct Remotes {
    hosts: Box<dyn Hosts>,
    projects: Mutex<HashMap<Location, Arc<Slot>>>,
    /// Each host's spawner, with the client it was connected through.
    spawners: Mutex<HashMap<HostId, (Client, Connecting)>>,
}

struct Slot {
    opening: futures::lock::Mutex<()>,
    held: Mutex<Option<(Client, remote::Project)>>,
    cancel: Mutex<Option<futures::channel::oneshot::Sender<()>>>,
    closed: Shared<BoxFuture<'static, ()>>,
}

impl Default for Slot {
    fn default() -> Self {
        let (cancel, closed) = futures::channel::oneshot::channel();
        Self {
            opening: Default::default(),
            held: Default::default(),
            cancel: Mutex::new(Some(cancel)),
            closed: async move {
                let _ = closed.await;
            }
            .boxed()
            .shared(),
        }
    }
}

/// A spawner being connected, or connected; the error of one that failed.
type Connecting = Shared<BoxFuture<'static, Result<Arc<spawner::Remote>, String>>>;

impl Remotes {
    /// Makes `hosts` the process's. Refused once hosts are installed.
    pub fn install(hosts: impl Hosts) -> anyhow::Result<()> {
        let remotes = Self {
            hosts: Box::new(hosts),
            projects: Mutex::default(),
            spawners: Mutex::default(),
        };
        REMOTES
            .set(remotes)
            .map_err(|_| anyhow::anyhow!("hosts are already installed"))
    }

    /// The installed hosts. Refused before [`Remotes::install`].
    pub fn get() -> anyhow::Result<&'static Self> {
        REMOTES
            .get()
            .context("no hosts are reachable from this build")
    }

    /// The client of `host`'s session.
    pub fn client(&self, host: HostId) -> anyhow::Result<Client> {
        self.hosts.client(host)
    }

    /// The commands through `host`'s ssh connection, while it has one.
    #[cfg(unix)]
    pub fn control(&self, host: HostId) -> Option<Control> {
        self.hosts.control(host)
    }

    /// The project at `location`, filled, and kept from then on: the one
    /// already open there, or the one opened now. Refused for a location on
    /// this machine.
    pub async fn open(&self, location: &Location) -> anyhow::Result<remote::Project> {
        let host = location
            .host()
            .with_context(|| format!("{location} is on this machine"))?;
        let slot = self.projects().entry(location.clone()).or_default().clone();
        let opening = async {
            let _opening = slot.opening.lock().await;
            anyhow::ensure!(
                self.projects()
                    .get(location)
                    .is_some_and(|held| Arc::ptr_eq(held, &slot)),
                "project was closed while opening: {location}"
            );
            let client = self.client(host)?;
            let held = slot.held.lock().unwrap().clone();
            let (project, refresh) = match held {
                Some((connected, project)) if connected.same(&client) => return Ok(project),
                Some((_, project)) => {
                    let refresh = project
                        .reconnect(client.clone())
                        .await
                        .with_context(|| format!("reopen {location}"))?;
                    (project, refresh)
                }
                None => remote::Project::open(client.clone(), location.root().to_owned())
                    .await
                    .with_context(|| format!("open {location}"))?,
            };
            let projects = self.projects();
            anyhow::ensure!(
                projects
                    .get(location)
                    .is_some_and(|held| Arc::ptr_eq(held, &slot)),
                "project was closed while opening: {location}"
            );
            *slot.held.lock().unwrap() = Some((client, project.clone()));
            acp::runtime().spawn(refresh);
            Ok(project)
        };
        match futures::future::select(Box::pin(opening), slot.closed.clone()).await {
            futures::future::Either::Left((opened, _)) => opened,
            futures::future::Either::Right(_) => {
                anyhow::bail!("project was closed while opening: {location}")
            }
        }
    }

    /// The project open at `location`.
    pub fn project(&self, location: &Location) -> Option<remote::Project> {
        self.projects().get(location).and_then(|slot| {
            slot.held
                .lock()
                .unwrap()
                .as_ref()
                .map(|(_, project)| project.clone())
        })
    }

    /// Lets the project at `location` go. Its replica stops being refreshed
    /// once every clone of it is dropped.
    pub fn close(&self, location: &Location) {
        let mut projects = self.projects();
        if let Some(slot) = projects.remove(location)
            && let Some(cancel) = slot.cancel.lock().unwrap().take()
        {
            let _ = cancel.send(());
        }
        if let Some(host) = location.host()
            && !projects
                .keys()
                .any(|location| location.host() == Some(host))
        {
            self.spawners.lock().unwrap().remove(&host);
        }
    }

    /// `host`'s spawner, connected once per client of the host. A failed
    /// connection is tried again by the next call.
    pub async fn spawner(&self, host: HostId) -> io::Result<Arc<spawner::Remote>> {
        let client = self.client(host).map_err(io::Error::other)?;
        let connecting = {
            let mut spawners = self.spawners.lock().unwrap_or_else(|e| e.into_inner());
            match spawners.get(&host) {
                Some((held, connecting)) if held.same(&client) => connecting.clone(),
                _ => {
                    let exec = client.service();
                    let connect = acp::runtime().spawn(spawner::Remote::connect(exec));
                    let connecting = async move {
                        match connect.await {
                            Ok(connected) => connected.map(Arc::new).map_err(|e| e.to_string()),
                            Err(e) => Err(e.to_string()),
                        }
                    }
                    .boxed()
                    .shared();
                    spawners.insert(host, (client.clone(), connecting.clone()));
                    connecting
                }
            }
        };
        let connected = connecting.await;
        if connected.is_err() {
            let mut spawners = self.spawners.lock().unwrap_or_else(|e| e.into_inner());
            if spawners
                .get(&host)
                .is_some_and(|(held, _)| held.same(&client))
            {
                spawners.remove(&host);
            }
        }
        connected.map_err(io::Error::other)
    }

    fn projects(&self) -> MutexGuard<'_, HashMap<Location, Arc<Slot>>> {
        self.projects.lock().unwrap_or_else(|e| e.into_inner())
    }
}
