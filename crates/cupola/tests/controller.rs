//! A controller over a mock connector: every session's transport opens links
//! to one in-process server, or refuses every open.

#![cfg(unix)]

use std::{
    os::unix::fs::PermissionsExt as _,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};

use anyhow::anyhow;
use cydonia_cupola::{
    Client, Error, Server, Service,
    client::{
        controller::{Changes, Connector, Controller, HostStatus},
        host::HostId,
        session::{Heartbeat, Link, Mode, Status, Transport},
        ssh::Host,
    },
    mock::Wire,
    ping::Ping,
};
use futures::{FutureExt as _, StreamExt as _, future::BoxFuture};

const HEARTBEAT: Heartbeat = Heartbeat {
    interval: Duration::from_millis(20),
    timeout: Duration::from_millis(100),
    missed: 2,
};
const WAIT: Duration = Duration::from_secs(10);
const REFUSAL: &str = "the host refused";

/// A request the server never answers, and an event it never pushes.
struct Stall;

impl Service for Stall {
    const NAME: &'static str = "stall";
    type Request = ();
    type Response = ();
    type Event = ();
}

/// Answered by every client the controller makes.
struct Whoami;

impl Service for Whoami {
    const NAME: &'static str = "whoami";
    type Request = ();
    type Response = String;
    type Event = ();
}

/// Hands out transports to `server`, counting them.
struct Mock {
    server: Server,
    refuse: bool,
    made: Arc<AtomicUsize>,
    wires: Arc<Mutex<Vec<Wire>>>,
}

struct Served {
    server: Server,
    /// Every open fails with [`REFUSAL`].
    refuse: bool,
    wires: Arc<Mutex<Vec<Wire>>>,
}

/// What a test keeps of its mock once the controller owns it.
struct Probe {
    server: Server,
    made: Arc<AtomicUsize>,
    wires: Arc<Mutex<Vec<Wire>>>,
}

impl Connector for Mock {
    type Transport = Served;

    fn transport(&self, _: &Host) -> anyhow::Result<Served> {
        self.made.fetch_add(1, Ordering::SeqCst);
        Ok(Served {
            server: self.server.clone(),
            refuse: self.refuse,
            wires: self.wires.clone(),
        })
    }

    fn serve(&self, client: &Client) {
        client.register::<Whoami, _, _>(|()| async { Ok("client".to_owned()) });
    }
}

impl Transport for Served {
    fn open(&self, _: Mode) -> BoxFuture<'_, anyhow::Result<Link>> {
        if self.refuse {
            return futures::future::ready(Err(anyhow!(REFUSAL))).boxed();
        }
        let (wire, near, far) = Wire::pair();
        let server = self.server.clone();
        tokio::spawn(async move {
            let _ = server.attach(far).await;
        });
        self.wires.lock().unwrap().push(wire);
        futures::future::ready(Ok(Link {
            connection: near,
            exit: futures::future::pending().boxed(),
        }))
        .boxed()
    }
}

impl Probe {
    fn made(&self) -> usize {
        self.made.load(Ordering::SeqCst)
    }

    fn sever(&self) {
        self.wires.lock().unwrap().last().unwrap().sever();
    }
}

/// A controller over the hosts file at `path`, its runner on a tokio task.
fn open(path: &Path) -> (Controller<Mock>, Probe) {
    open_with(path, false)
}

fn open_with(path: &Path, refuse: bool) -> (Controller<Mock>, Probe) {
    let (server, runner) = Server::new();
    tokio::spawn(runner);
    server.register::<Stall, _, _>(|()| futures::future::pending());
    let mock = Mock {
        server,
        refuse,
        made: Arc::default(),
        wires: Arc::default(),
    };
    let probe = Probe {
        server: mock.server.clone(),
        made: mock.made.clone(),
        wires: mock.wires.clone(),
    };
    let (controller, runner) = Controller::open(path, mock, HEARTBEAT).unwrap();
    tokio::spawn(runner);
    (controller, probe)
}

fn hosts_file(dir: &tempfile::TempDir) -> PathBuf {
    dir.path().join("cupola").join("hosts.toml")
}

fn names(controller: &Controller<Mock>) -> Vec<(HostId, String, String)> {
    controller
        .hosts()
        .iter()
        .map(|saved| {
            (
                saved.id(),
                saved.name().to_owned(),
                saved.host().to_string(),
            )
        })
        .collect()
}

fn at(id: HostId, status: Status) -> (HostId, HostStatus) {
    (id, status.into())
}

/// The changes up to and including the first of `id` to `status`.
async fn until(changes: &mut Changes, id: HostId, status: Status) -> Vec<(HostId, HostStatus)> {
    let mut seen = Vec::new();
    tokio::time::timeout(WAIT, async {
        while let Some(next) = changes.next().await {
            let last = next.0 == id && next.1.status() == status;
            seen.push(next);
            if last {
                return;
            }
        }
    })
    .await
    .unwrap_or_else(|_| panic!("no {status:?} after {seen:?}"));
    seen
}

#[tokio::test]
async fn hosts_round_trip_through_their_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = hosts_file(&dir);
    let (controller, _) = open(&path);
    assert!(controller.hosts().is_empty());

    let build = controller
        .add("build box", Host::new("me@build").unwrap().port(2222))
        .unwrap();
    let pi = controller.add(" pi ", Host::new("pi").unwrap()).unwrap();
    let gone = controller.add("gone", Host::new("gone").unwrap()).unwrap();
    controller.rename(build, "builder").unwrap();
    controller.remove(gone).unwrap();
    let saved = names(&controller);
    assert_eq!(
        saved,
        [
            (build, "builder".to_owned(), "me@build:2222".to_owned()),
            (pi, "pi".to_owned(), "pi".to_owned()),
        ]
    );
    drop(controller);

    let (reopened, _) = open(&path);
    assert_eq!(names(&reopened), saved);
    let next = reopened.add("next", Host::new("next").unwrap()).unwrap();
    assert!(![build, pi, gone].contains(&next));
}

#[tokio::test]
async fn reading_normalizes_without_writing() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("hosts.toml");
    let text = r#"
[[host]]
id = 4
destination = "me@box"
port = 22

[[host]]
id = 4
name = "twin"
destination = "twin"

[[host]]
id = 2
name = "  pi  "
destination = "pi"
"#;
    std::fs::write(&path, text).unwrap();

    let (controller, _) = open(&path);
    let saved = names(&controller);
    assert_eq!(saved.len(), 2);
    assert_eq!(
        (saved[0].1.as_str(), saved[0].2.as_str()),
        ("me@box:22", "me@box:22")
    );
    assert_eq!((saved[1].1.as_str(), saved[1].2.as_str()), ("pi", "pi"));
    assert_eq!(std::fs::read_to_string(&path).unwrap(), text);

    let added = controller.add("new", Host::new("new").unwrap()).unwrap();
    assert!(![saved[0].0, saved[1].0].contains(&added));
}

#[tokio::test]
async fn a_bad_destination_fails_the_read() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("hosts.toml");
    std::fs::write(&path, "[[host]]\nid = 1\ndestination = \"-oEvil\"\n").unwrap();
    let mock = Mock {
        server: Server::new().0,
        refuse: false,
        made: Arc::default(),
        wires: Arc::default(),
    };
    assert!(Controller::open(&path, mock, HEARTBEAT).is_err());
}

#[tokio::test]
async fn writes_replace_the_file_whole_or_not_at_all() {
    let dir = tempfile::tempdir().unwrap();
    let path = hosts_file(&dir);
    let (controller, _) = open(&path);
    let kept = controller.add("kept", Host::new("kept").unwrap()).unwrap();
    controller.add("more", Host::new("more").unwrap()).unwrap();
    let entries: Vec<_> = std::fs::read_dir(path.parent().unwrap())
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect();
    assert_eq!(entries, ["hosts.toml"]);

    let before = std::fs::read_to_string(&path).unwrap();
    let listed = names(&controller);
    let parent = path.parent().unwrap();
    std::fs::set_permissions(parent, std::fs::Permissions::from_mode(0o555)).unwrap();
    let added = controller.add("refused", Host::new("refused").unwrap());
    let renamed = controller.rename(kept, "refused");
    let removed = controller.remove(kept);
    std::fs::set_permissions(parent, std::fs::Permissions::from_mode(0o755)).unwrap();

    assert!(added.is_err() && renamed.is_err() && removed.is_err());
    assert_eq!(std::fs::read_to_string(&path).unwrap(), before);
    assert_eq!(names(&controller), listed);
}

#[tokio::test]
async fn names_must_not_be_empty_and_ids_must_be_known() {
    let dir = tempfile::tempdir().unwrap();
    let (controller, _) = open(&hosts_file(&dir));
    assert!(controller.add("  ", Host::new("box").unwrap()).is_err());
    let id = controller.add("box", Host::new("box").unwrap()).unwrap();
    assert!(controller.rename(id, "").is_err());
    controller.remove(id).unwrap();
    assert!(controller.rename(id, "box").is_err());
    assert!(controller.remove(id).is_err());
    assert!(controller.client(id).is_err());
}

#[tokio::test(flavor = "multi_thread")]
async fn every_caller_gets_the_one_client_of_a_host() {
    let dir = tempfile::tempdir().unwrap();
    let (controller, probe) = open(&hosts_file(&dir));
    let first = controller
        .add("first", Host::new("first").unwrap())
        .unwrap();
    let second = controller
        .add("second", Host::new("second").unwrap())
        .unwrap();

    let one = controller.client(first).unwrap();
    let other = controller.clone().client(first).unwrap();
    assert_eq!(probe.made(), 1);
    for client in [&one, &other] {
        let ping = client.service::<Ping>();
        tokio::time::timeout(WAIT, ping.request(&()))
            .await
            .unwrap()
            .unwrap();
    }

    controller.client(second).unwrap();
    assert_eq!(probe.made(), 2);

    controller.disconnect(first);
    controller.client(first).unwrap();
    controller.client(first).unwrap();
    assert_eq!(probe.made(), 3);
}

#[tokio::test(flavor = "multi_thread")]
async fn statuses_reach_every_subscriber() {
    let dir = tempfile::tempdir().unwrap();
    let (controller, probe) = open(&hosts_file(&dir));
    let id = controller.add("box", Host::new("box").unwrap()).unwrap();

    let (now, mut one) = controller.statuses();
    let (_, mut other) = controller.statuses();
    assert_eq!(now, [at(id, Status::Disconnected)]);

    controller.client(id).unwrap();
    for changes in [&mut one, &mut other] {
        assert_eq!(
            until(changes, id, Status::Connected).await,
            [at(id, Status::Connecting), at(id, Status::Connected)]
        );
    }

    probe.sever();
    for changes in [&mut one, &mut other] {
        assert_eq!(
            until(changes, id, Status::Connected).await,
            [at(id, Status::Reconnecting), at(id, Status::Connected)]
        );
    }

    controller.disconnect(id);
    for changes in [&mut one, &mut other] {
        assert_eq!(
            until(changes, id, Status::Disconnected).await,
            [at(id, Status::Disconnected)]
        );
    }
    assert_eq!(controller.status(id), Status::Disconnected.into());
}

#[tokio::test(flavor = "multi_thread")]
async fn a_session_that_gives_up_reports_why() {
    let dir = tempfile::tempdir().unwrap();
    let (controller, _) = open_with(&hosts_file(&dir), true);
    let id = controller.add("box", Host::new("box").unwrap()).unwrap();
    let (_, mut one) = controller.statuses();
    let (_, mut other) = controller.statuses();

    let held = controller.client(id).unwrap();
    for changes in [&mut one, &mut other] {
        let seen = until(changes, id, Status::Disconnected).await;
        assert_eq!(seen.last().unwrap().1.failure(), Some(REFUSAL));
    }
    let (now, _) = controller.statuses();
    assert_eq!(now[0].1.failure(), Some(REFUSAL));
    assert!(matches!(
        held.service::<Ping>().request(&()).await,
        Err(Error::Closed)
    ));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_held_client_is_closed_by_disconnect() {
    let dir = tempfile::tempdir().unwrap();
    let (controller, _) = open(&hosts_file(&dir));
    let id = controller.add("box", Host::new("box").unwrap()).unwrap();
    let (_, mut changes) = controller.statuses();
    let held = controller.client(id).unwrap();
    until(&mut changes, id, Status::Connected).await;

    let stall = held.service::<Stall>();
    let mut events = stall.events();
    let waiting = tokio::spawn({
        let stall = stall.clone();
        async move { stall.request(&()).await }
    });
    tokio::time::sleep(Duration::from_millis(50)).await;
    controller.disconnect(id);

    let waiting = tokio::time::timeout(WAIT, waiting).await.unwrap().unwrap();
    assert!(matches!(waiting, Err(Error::Closed)));
    assert!(matches!(stall.request(&()).await, Err(Error::Closed)));
    assert!(
        tokio::time::timeout(WAIT, events.next())
            .await
            .unwrap()
            .is_none()
    );
    assert!(stall.events().next().await.is_none());
    assert_eq!(controller.status(id), Status::Disconnected.into());
}

#[tokio::test(flavor = "multi_thread")]
async fn removing_a_connected_host_disconnects_it() {
    let dir = tempfile::tempdir().unwrap();
    let (controller, _) = open(&hosts_file(&dir));
    let id = controller.add("box", Host::new("box").unwrap()).unwrap();
    let (_, mut changes) = controller.statuses();
    let held = controller.client(id).unwrap();
    until(&mut changes, id, Status::Connected).await;

    controller.remove(id).unwrap();
    assert_eq!(
        until(&mut changes, id, Status::Disconnected).await,
        [at(id, Status::Disconnected)]
    );
    assert!(controller.statuses().0.is_empty());
    assert!(matches!(
        held.service::<Ping>().request(&()).await,
        Err(Error::Closed)
    ));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_server_that_calls_the_client_on_attach_gets_an_answer() {
    let dir = tempfile::tempdir().unwrap();
    let (controller, probe) = open(&hosts_file(&dir));
    let id = controller.add("box", Host::new("box").unwrap()).unwrap();

    let asked = probe.server.service::<Whoami>().send(&());
    controller.client(id).unwrap();
    let answer = tokio::time::timeout(WAIT, asked).await.unwrap();
    assert_eq!(answer.unwrap(), "client");
}
