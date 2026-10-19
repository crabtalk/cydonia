//! A daemon in a process of its own, reached through proxies run on threads
//! of the test, each bridging one end of a socket pair as ssh's stdio would.
//!
//! The daemon is this test binary run again on [`daemon_process`], with the
//! state directory in [`DAEMON_DIR`].

#![cfg(unix)]

use std::{
    os::unix::net::UnixStream,
    path::{Path, PathBuf},
    process::Command,
    thread,
    time::{Duration, Instant},
};

use cydonia_cupola::{
    Client, Connection, Server, Service,
    daemon::Daemon,
    exec::{self, Exec, Output},
    fs::{Expect, Fs, Request, Response},
    proxy::{self, Launch, NO_DAEMON},
};
use futures::StreamExt as _;
use tokio::task::JoinHandle;

const DAEMON_DIR: &str = "CUPOLA_TEST_DAEMON_DIR";
const PATIENCE: Duration = Duration::from_secs(10);

/// A state directory whose daemon is stopped on drop.
struct Home(tempfile::TempDir);

/// One proxy's thread, and the client's attachment through it.
struct Link {
    proxy: thread::JoinHandle<Result<(), proxy::Error>>,
    attached: JoinHandle<()>,
}

/// Runs the daemon when this binary was started as one, and passes otherwise.
#[test]
fn daemon_process() {
    let Some(dir) = std::env::var_os(DAEMON_DIR) else {
        return;
    };
    let (server, runner) = Server::new();
    Fs::register(&server);
    Exec::register(&server);
    let daemon = Daemon::at(dir);
    let (_, served) =
        futures::executor::block_on(futures::future::join(runner, daemon.run(&server)));
    served.unwrap();
}

impl Home {
    fn new() -> Self {
        Self(tempfile::tempdir().unwrap())
    }

    fn path(&self) -> &Path {
        self.0.path()
    }

    fn start(&self) -> Launch {
        let mut command = Command::new(std::env::current_exe().unwrap());
        command
            .args(["daemon_process", "--exact", "--nocapture"])
            .env(DAEMON_DIR, self.path());
        Launch::Start(command)
    }
}

impl Drop for Home {
    fn drop(&mut self) {
        let _ = Daemon::at(self.path()).stop();
    }
}

impl Link {
    /// Starts a proxy in `dir` and attaches `client` through it.
    fn open(dir: &Path, launch: Launch, client: &Client) -> Self {
        let (near, far) = UnixStream::pair().unwrap();
        let dir = PathBuf::from(dir);
        let proxy = thread::spawn(move || {
            let input = far.try_clone().unwrap();
            Daemon::at(dir).proxy(launch, input, far)
        });
        let near = async_net::unix::UnixStream::try_from(near).unwrap();
        let client = client.clone();
        let attached = tokio::spawn(async move {
            let _ = client.attach(Connection::new(near.clone(), near)).await;
        });
        Self { proxy, attached }
    }

    /// Drops the client's end, and answers with how the proxy ended.
    async fn close(self) -> Result<(), proxy::Error> {
        self.attached.abort();
        let _ = self.attached.await;
        let proxy = self.proxy;
        tokio::task::spawn_blocking(move || proxy.join().unwrap())
            .await
            .unwrap()
    }
}

#[test]
fn reconnecting_with_no_daemon_exits_with_its_own_code() {
    let home = Home::new();
    let error = Daemon::at(home.path())
        .proxy(Launch::Reconnect, std::io::empty(), std::io::sink())
        .unwrap_err();
    assert!(matches!(error, proxy::Error::NoDaemon), "{error:?}");
    assert_eq!(error.exit_code(), NO_DAEMON);
    assert_eq!(NO_DAEMON, 90);
}

#[tokio::test(flavor = "multi_thread")]
async fn the_daemon_outlives_a_proxy_and_takes_the_client_back() {
    let home = Home::new();
    let (client, _) = Client::new();
    let file = home.path().join("note");

    let first = Link::open(home.path(), home.start(), &client);
    let written = client
        .service::<Fs>()
        .request(&Request::Write {
            path: file.clone(),
            bytes: b"kept".to_vec(),
            expect: Expect::Anything,
        })
        .await
        .unwrap();
    assert_eq!(written, Response::Written);
    let mut command = exec::Command::new("/bin/sh");
    command.args = vec!["-c".into(), r#"read line; echo "got $line""#.into()];
    let mut process = client.service::<Exec>().start(&command).await.unwrap();
    first.close().await.unwrap();

    let second = Link::open(home.path(), Launch::Reconnect, &client);
    process.write(b"later\n").await.unwrap();
    let read = client
        .service::<Fs>()
        .request(&Request::Read {
            path: file,
            limit: None,
        })
        .await
        .unwrap();
    assert_eq!(
        read,
        Response::Read {
            bytes: b"kept".to_vec(),
            cut: false,
        }
    );
    assert_eq!(
        process.next().await.unwrap().unwrap(),
        Output::Stdout(b"got later\n".to_vec())
    );
    assert_eq!(
        process.next().await.unwrap().unwrap(),
        Output::Exit(Some(0))
    );

    Daemon::at(home.path()).stop().unwrap();
    let ended = second.close().await.unwrap_err();
    assert_eq!(ended.exit_code(), NO_DAEMON);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_fresh_start_replaces_the_daemon_running() {
    let home = Home::new();
    let (first_client, _) = Client::new();
    let first = Link::open(home.path(), home.start(), &first_client);
    let mut command = exec::Command::new("/bin/sh");
    command.args = vec!["-c".into(), "echo $PPID".into()];
    let first_daemon = daemon_pid(&first_client, &command).await;

    let (second_client, _) = Client::new();
    let second = Link::open(home.path(), home.start(), &second_client);
    let second_daemon = daemon_pid(&second_client, &command).await;
    assert_ne!(first_daemon, second_daemon);

    let _ = first.close().await;
    Daemon::at(home.path()).stop().unwrap();
    let _ = second.close().await;
}

/// Shouts back what it is sent.
struct Shout;

impl Service for Shout {
    const NAME: &'static str = "shout";
    type Request = String;
    type Response = String;
    type Event = ();
}

/// The daemon runs in this process, so nothing here stops it: stopping
/// signals the pid in its pid file.
#[tokio::test(flavor = "multi_thread")]
async fn processes_that_join_are_each_served_by_a_server_of_their_own() {
    let dir = tempfile::tempdir().unwrap();
    let missing = Daemon::at(dir.path()).join().await;
    assert!(missing.is_err_and(|error| error.kind() == std::io::ErrorKind::NotFound));

    let (server, runner) = Server::new();
    tokio::spawn(runner);
    let state = dir.path().to_owned();
    tokio::spawn(async move {
        let daemon = Daemon::at(state);
        let welcomed = daemon.welcome(|local| {
            local.register::<Shout, _, _>(|said: String| async move { Ok(said.to_uppercase()) })
        });
        let _ = futures::future::join(daemon.run(&server), welcomed).await;
    });

    let started = Instant::now();
    let mut shouted = Vec::new();
    for said in ["hi", "there"] {
        let connection = loop {
            match Daemon::at(dir.path()).join().await {
                Ok(connection) => break connection,
                Err(error) => assert!(started.elapsed() < PATIENCE, "{error}"),
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        };
        let (client, runner) = Client::new();
        tokio::spawn(runner);
        let attached = client.clone();
        tokio::spawn(async move { attached.attach(connection).await });
        shouted.push(
            client
                .service::<Shout>()
                .request(&said.to_owned())
                .await
                .unwrap(),
        );
    }
    assert_eq!(shouted, ["HI", "THERE"]);
}

/// The pid of the daemon that ran `command`, as the command printed it.
async fn daemon_pid(client: &Client, command: &exec::Command) -> String {
    let mut process = client.service::<Exec>().start(command).await.unwrap();
    match process.next().await.unwrap().unwrap() {
        Output::Stdout(bytes) => String::from_utf8(bytes).unwrap(),
        other => panic!("expected stdout, got {other:?}"),
    }
}
