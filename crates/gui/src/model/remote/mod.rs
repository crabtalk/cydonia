//! The hosts the app reaches projects on, and the projects it has opened on
//! them: one [`Remotes`] for the process, installed once over the [`Hosts`]
//! that hand out each host's client. Nothing is reachable on another machine
//! before [`Remotes::install`].
//!
//! Every per-project pick that routes a [`Location::Remote`] asks here: the
//! store, git, the files and the spawner.

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
    projects: Mutex<HashMap<Location, remote::Project>>,
    /// Each host's spawner, with the client it was connected through.
    spawners: Mutex<HashMap<HostId, (Client, Connecting)>>,
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
        if let Some(project) = self.project(location) {
            return Ok(project);
        }
        let host = location
            .host()
            .with_context(|| format!("{location} is on this machine"))?;
        let client = self.client(host)?;
        let (project, refresh) = remote::Project::open(client, location.root().to_owned())
            .await
            .with_context(|| format!("open {location}"))?;
        let mut projects = self.projects();
        if let Some(held) = projects.get(location) {
            return Ok(held.clone());
        }
        acp::runtime().spawn(refresh);
        projects.insert(location.clone(), project.clone());
        Ok(project)
    }

    /// The project open at `location`.
    pub fn project(&self, location: &Location) -> Option<remote::Project> {
        self.projects().get(location).cloned()
    }

    /// Lets the project at `location` go. Its replica stops being refreshed
    /// once every clone of it is dropped.
    pub fn close(&self, location: &Location) {
        self.projects().remove(location);
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

    fn projects(&self) -> MutexGuard<'_, HashMap<Location, remote::Project>> {
        self.projects.lock().unwrap_or_else(|e| e.into_inner())
    }
}
