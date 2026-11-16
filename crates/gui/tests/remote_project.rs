//! A remote project opened from its `Location` through `Remotes`, over a
//! Cupola controller whose sessions reach cydonia-server's services in this
//! process over the mock transport, and the form a remote location is
//! written in.

#![cfg(unix)]

use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use artifact::{
    location::Location,
    project::{Project as _, fs},
};
use cupola::{
    Client, Server,
    client::{
        controller::{Connector, Controller},
        host::HostId,
        session::{Heartbeat, Link, Mode, Transport},
        ssh::{Control, Host},
    },
    mock::Wire,
};
use cydonia_gui::{
    agent::acp,
    model::{
        disk::{Backend, Files as _},
        git,
        remote::{Hosts, Remotes},
        state::State,
        store::{self, Store},
    },
};
use futures::{FutureExt as _, future::BoxFuture};
use server::Services;

/// Opens every session's links to one in-process server.
struct Mock {
    ready: futures::future::Shared<BoxFuture<'static, ()>>,
}

struct Served {
    server: Server,
    ready: futures::future::Shared<BoxFuture<'static, ()>>,
}

/// The controller as the app holds it, with no ssh connection behind it.
struct Probe(Controller<Mock>);

impl Connector for Mock {
    type Transport = Served;

    fn transport(&self, host: &Host) -> anyhow::Result<Served> {
        let (server, runner) = Server::new();
        Services::register(&server);
        acp::runtime().spawn(runner);
        let ready = if host.destination() == "slow" {
            self.ready.clone()
        } else {
            async {}.boxed().shared()
        };
        Ok(Served { server, ready })
    }
}

impl Transport for Served {
    fn open(&self, _: Mode) -> BoxFuture<'_, anyhow::Result<Link>> {
        async move {
            self.ready.clone().await;
            let (wire, near, far) = Wire::pair();
            let server = self.server.clone();
            acp::runtime().spawn(async move {
                let _wire: Wire = wire;
                let _ = server.attach(far).await;
            });
            Ok(Link {
                connection: near,
                exit: futures::future::pending().boxed(),
            })
        }
        .boxed()
    }
}

impl Hosts for Probe {
    fn client(&self, host: HostId) -> anyhow::Result<Client> {
        self.0.client(host)
    }

    fn control(&self, _: HostId) -> Option<Control> {
        None
    }
}

/// A controller with one saved host whose sessions reach a server answering
/// cydonia's services, installed as the process's [`Remotes`]. Answers the
/// host.
fn install(hosts: &Path) -> (HostId, Controller<Mock>) {
    install_with(hosts, async {}.boxed().shared())
}

fn install_with(
    hosts: &Path,
    ready: futures::future::Shared<BoxFuture<'static, ()>>,
) -> (HostId, Controller<Mock>) {
    let runtime = acp::runtime();
    let (controller, runner) = Controller::open(
        hosts.join("hosts.toml"),
        Mock { ready },
        Heartbeat::default(),
    )
    .unwrap();
    runtime.spawn(runner);
    let host = controller.add("box", Host::new("box").unwrap()).unwrap();
    Remotes::install(Probe(controller.clone())).unwrap();
    (host, controller)
}

#[test]
fn a_remote_location_opens_and_every_pick_reaches_its_host() {
    let scratch = tempfile::tempdir().unwrap();
    let root = scratch.path().join("project");
    std::fs::create_dir(&root).unwrap();
    let made =
        futures::executor::block_on(fs::Project::new(&root).create_board("Plans", "PLAN")).unwrap();
    // The login environment the remote spawner reads.
    // SAFETY: before this test starts a thread.
    unsafe { std::env::set_var("SHELL", "/bin/sh") };
    let (host, _) = install(scratch.path());
    let location = Location::Remote {
        host,
        root: root.clone(),
    };
    let remotes = Remotes::get().unwrap();
    assert!(store::open(&location).is_none(), "nothing open yet");

    let runtime = acp::runtime();
    runtime.block_on(remotes.open(&location)).unwrap();
    let Some(Store::Remote(opened)) = store::open(&location) else {
        panic!("the remote store");
    };
    assert_eq!(opened.root(), root);
    assert_eq!(
        opened.board(&made.id).map(|board| board.name),
        Some("Plans".into())
    );

    let Backend::Remote(files) = Backend::open(&location) else {
        panic!("the remote files");
    };
    let listed = runtime.block_on(files.read_dir(&root)).unwrap();
    assert!(
        listed.contains(&(true, root.join(".cydonia"))),
        "{listed:?}"
    );

    let repository = runtime.block_on(git::open(&location).status(&root));
    assert!(repository.is_ok(), "{repository:?}");

    let first = runtime.block_on(remotes.spawner(host)).unwrap();
    let again = runtime.block_on(remotes.spawner(host)).unwrap();
    assert!(Arc::ptr_eq(&first, &again), "one spawner per host");

    remotes.close(&location);
    assert!(store::open(&location).is_none(), "closed");

    let elsewhere = Location::Remote {
        host: "999".parse().unwrap(),
        root: PathBuf::from("/nowhere"),
    };
    assert!(runtime.block_on(remotes.open(&elsewhere)).is_err());
    let unreachable = Backend::open(&elsewhere);
    assert!(
        runtime
            .block_on(unreachable.read_dir(Path::new("/")))
            .is_err()
    );
    assert!(
        runtime
            .block_on(git::open(&elsewhere).status(Path::new("/")))
            .is_err()
    );
}

#[test]
fn a_remote_location_is_written_as_cupola_and_a_local_one_as_its_path() {
    let remote = Location::Remote {
        host: "3".parse().unwrap(),
        root: PathBuf::from("/home/me/work"),
    };
    let local = Location::Local(PathBuf::from("/work/one"));
    let state = State {
        projects: vec![local.clone(), remote.clone()],
        collapsed: vec![remote.clone()],
        ..State::default()
    };
    let written = toml::to_string(&state).unwrap();
    assert!(
        written.contains(r#"projects = ["/work/one", "cupola://3/home/me/work"]"#),
        "{written}"
    );
    let read: State = toml::from_str(&written).unwrap();
    assert_eq!(read.projects, state.projects);
    assert_eq!(read.collapsed, state.collapsed);

    // Not a well-formed remote location: a local path, as before.
    let read: Location = serde_json::from_str(r#""cupola://box/x""#).unwrap();
    assert_eq!(read, Location::Local(PathBuf::from("cupola://box/x")));
}

#[test]
fn reopening_after_a_disconnect_refreshes_held_stores_and_replaces_the_spawner() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;

    let scratch = tempfile::tempdir().unwrap();
    let root = scratch.path().join("project");
    std::fs::create_dir(&root).unwrap();
    // SAFETY: before the runtime starts any thread.
    unsafe { std::env::set_var("SHELL", "/bin/sh") };
    let (host, controller) = install(scratch.path());
    let location = Location::Remote {
        host,
        root: root.clone(),
    };
    let remotes = Remotes::get().unwrap();
    acp::runtime().block_on(async {
        tokio::time::timeout(Duration::from_secs(10), async {
            let held = remotes.open(&location).await.unwrap();
            let before = controller.client(host).unwrap();
            let spawner = remotes.spawner(host).await.unwrap();
            let knocked = Arc::new(AtomicUsize::new(0));
            let count = knocked.clone();
            let _watch = held.watch(move || {
                count.fetch_add(1, Ordering::SeqCst);
            });
            controller.disconnect(host);
            let made = fs::Project::new(&root)
                .create_board("While disconnected", "OFF")
                .await
                .unwrap();
            remotes.open(&location).await.unwrap();
            assert!(!before.same(&controller.client(host).unwrap()));
            assert_eq!(held.board(&made.id).unwrap().name, "While disconnected");
            assert!(knocked.load(Ordering::SeqCst) > 0);
            // A clone held before reconnect writes through the replacement client.
            let next = held.create_board("After reconnect", "ON").await.unwrap();
            assert!(fs::Project::new(&root).board(&next.id).is_some());
            assert!(!Arc::ptr_eq(
                &spawner,
                &remotes.spawner(host).await.unwrap()
            ));
        })
        .await
        .expect("reopen must finish rather than wait on a closed client");
    });
}

#[test]
fn slow_open_does_not_block_other_hosts_and_close_prevents_a_late_open() {
    use std::time::Duration;
    let scratch = tempfile::tempdir().unwrap();
    let (release, ready) = futures::channel::oneshot::channel::<()>();
    let (fast, controller) = install_with(
        scratch.path(),
        async {
            ready.await.unwrap();
        }
        .boxed()
        .shared(),
    );
    let slow = controller.add("Slow", Host::new("slow").unwrap()).unwrap();
    let fast = Location::Remote {
        host: fast,
        root: scratch.path().join("fast"),
    };
    let slow = Location::Remote {
        host: slow,
        root: scratch.path().join("slow"),
    };
    let remotes = Remotes::get().unwrap();
    acp::runtime().block_on(async {
        tokio::time::timeout(Duration::from_secs(10), async {
            let opening = acp::runtime().spawn({
                let slow = slow.clone();
                async move { remotes.open(&slow).await }
            });
            while controller.status(slow.host().unwrap()).status()
                != cupola::client::session::Status::Connecting
            {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
            remotes.open(&fast).await.unwrap();
            remotes.close(&slow);
            let error = opening
                .await
                .unwrap()
                .err()
                .expect("a closed open cannot publish a project");
            assert!(
                error.to_string().contains("closed while opening"),
                "{error}"
            );
            assert!(remotes.project(&slow).is_none());
            release.send(()).unwrap();
            // A new open gets its own slot; closing another project leaves it held.
            remotes.open(&slow).await.unwrap();
            remotes.close(&fast);
            assert!(remotes.project(&fast).is_none());
            assert!(remotes.project(&slow).is_some());
            remotes.close(&slow);
        })
        .await
        .expect("a slow host must not hold another host's open");
    });
}
