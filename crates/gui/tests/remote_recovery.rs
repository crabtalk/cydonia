//! Project replicas across link loss, server replacement, and exhausted retries.
#![cfg(unix)]

use artifact::project::{Project as _, fs, remote};
use cupola::{
    Client, Server,
    client::session::{Exit, Heartbeat, Link, Mode, Session, Status, Transport},
    mock::Wire,
};
use futures::{FutureExt as _, StreamExt as _, future::BoxFuture};
use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
    time::Duration,
};

struct Network {
    servers: Mutex<VecDeque<Option<Server>>>,
    wires: Mutex<Vec<Wire>>,
}

impl Transport for Network {
    fn open(&self, _: Mode) -> BoxFuture<'_, anyhow::Result<Link>> {
        async move {
            let server = self
                .servers
                .lock()
                .unwrap()
                .pop_front()
                .ok_or_else(|| anyhow::anyhow!("host offline"))?;
            let (wire, near, far) = Wire::pair();
            self.wires.lock().unwrap().push(wire);
            let exit = match server {
                Some(server) => {
                    tokio::spawn(async move {
                        let _ = server.attach(far).await;
                    });
                    futures::future::pending().boxed()
                }
                None => {
                    drop(far);
                    futures::future::ready(Exit::NoDaemon).boxed()
                }
            };
            Ok(Link {
                connection: near,
                exit,
            })
        }
        .boxed()
    }
}

fn server() -> Server {
    let (server, runner) = Server::new();
    server::Services::register(&server);
    tokio::spawn(runner);
    server
}

async fn check(restart: bool) {
    tokio::time::timeout(Duration::from_secs(10), async {
        let scratch = tempfile::tempdir().unwrap();
        let root = scratch.path().to_owned();
        let disk = fs::Project::new(&root);
        let original = server();
        let plan = if restart {
            vec![Some(original), None, Some(server())]
        } else {
            vec![Some(original.clone()), Some(original)]
        };
        let (client, runner) = Client::new();
        tokio::spawn(runner);
        let session = Arc::new(Session::with_heartbeat(
            client.clone(),
            Network {
                servers: Mutex::new(plan.into()),
                wires: Mutex::default(),
            },
            Heartbeat {
                interval: Duration::from_millis(10),
                timeout: Duration::from_millis(50),
                missed: 2,
            },
        ));
        let mut statuses = session.statuses();
        let running = tokio::spawn({
            let session = session.clone();
            async move { session.run().await }
        });
        while statuses.next().await != Some(Status::Connected) {}
        let (project, refresh) = remote::Project::open(client, root).await.unwrap();
        let refreshed = tokio::spawn(refresh);
        session.transport().wires.lock().unwrap()[0].sever();
        // Changes made while the link is down must not be lost on recovery.
        let made = disk.create_board("During outage", "OUT").await.unwrap();
        while statuses.next().await != Some(Status::Connected) {}
        loop {
            if project.board(&made.id).is_some() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        let made = project
            .create_board("After recovery", "BACK")
            .await
            .unwrap();
        assert!(disk.board(&made.id).is_some());
        // Exhaust the remaining opens. The controller closes the client in
        // production; this direct session test checks it really terminates.
        session
            .transport()
            .wires
            .lock()
            .unwrap()
            .last()
            .unwrap()
            .sever();
        assert!(running.await.unwrap().is_err());
        refreshed.abort();
    })
    .await
    .expect("project recovery must finish");
}

#[test]
fn a_replica_recovers_after_a_severed_link() {
    acp_runtime().block_on(check(false));
}

#[test]
fn a_replica_refills_after_the_daemon_is_replaced() {
    acp_runtime().block_on(check(true));
}

fn acp_runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Runtime::new().unwrap()
}

#[test]
fn dropping_the_last_project_stops_its_refresh_on_a_live_connection() {
    acp_runtime().block_on(async {
        let scratch = tempfile::tempdir().unwrap();
        let server = server();
        let (client, runner) = Client::new();
        let runner = tokio::spawn(runner);
        let (_wire, near, far) = Wire::pair();
        let attached = client.clone();
        let near = tokio::spawn(async move { attached.attach(near).await });
        let far = tokio::spawn(async move { server.attach(far).await });
        let (project, refresh) = remote::Project::open(client, scratch.path().to_owned())
            .await
            .unwrap();
        let refresh = tokio::spawn(refresh);
        let copy = project.clone();
        let _watch = project.watch(|| {});
        drop(project);
        assert!(!refresh.is_finished(), "a held project keeps refreshing");
        drop(copy);
        tokio::time::timeout(Duration::from_secs(2), refresh)
            .await
            .expect("a closed project must not leave an idle refresh task")
            .unwrap();
        near.abort();
        far.abort();
        runner.abort();
    });
}
