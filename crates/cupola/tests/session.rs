//! A session over a mock transport: each open hands out one end of a mock
//! wire, as its plan says.

use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
    time::Duration,
};

use anyhow::anyhow;
use cydonia_cupola::{
    Client, Connection, Error, Server, Service,
    client::session::{Exit, Heartbeat, Link, Mode, Session, Status, Statuses, Transport},
    mock::Wire,
};
use futures::{FutureExt as _, StreamExt as _, channel::oneshot, future::BoxFuture};
use tokio::task::JoinHandle;

const HEARTBEAT: Heartbeat = Heartbeat {
    interval: Duration::from_millis(20),
    timeout: Duration::from_millis(100),
    missed: 2,
};
const WAIT: Duration = Duration::from_secs(10);

struct Echo;

/// Answered by the client, each call with the text its gate is opened with.
struct Gated;

impl Service for Gated {
    const NAME: &'static str = "gated";
    type Request = ();
    type Response = String;
    type Event = ();
}

impl Service for Echo {
    const NAME: &'static str = "echo";
    type Request = String;
    type Response = String;
    type Event = ();
}

/// What one open does.
enum Plan {
    /// A link to this server.
    Serve(Server),
    /// A link nothing reads from or answers on.
    Stall,
    /// A link that ends at once, telling no daemon was found.
    NoDaemon,
}

#[derive(Default)]
struct Mock {
    plan: Mutex<VecDeque<Plan>>,
    modes: Mutex<Vec<Mode>>,
    wires: Mutex<Vec<Wire>>,
    stalled: Mutex<Vec<Connection>>,
}

impl Mock {
    fn new(plan: impl IntoIterator<Item = Plan>) -> Self {
        Self {
            plan: Mutex::new(plan.into_iter().collect()),
            ..Self::default()
        }
    }

    fn modes(&self) -> Vec<Mode> {
        self.modes.lock().unwrap().clone()
    }

    fn sever(&self) {
        self.wires.lock().unwrap().last().unwrap().sever();
    }
}

impl Transport for Mock {
    fn open(&self, mode: Mode) -> BoxFuture<'_, anyhow::Result<Link>> {
        self.modes.lock().unwrap().push(mode);
        let plan = self.plan.lock().unwrap().pop_front();
        let link = plan.ok_or_else(|| anyhow!("unreachable")).map(|plan| {
            let (wire, near, far) = Wire::pair();
            let exit = match plan {
                Plan::Serve(server) => {
                    tokio::spawn(async move {
                        let _ = server.attach(far).await;
                    });
                    futures::future::pending().boxed()
                }
                Plan::Stall => {
                    self.stalled.lock().unwrap().push(far);
                    futures::future::pending().boxed()
                }
                Plan::NoDaemon => {
                    drop(far);
                    futures::future::ready(Exit::NoDaemon).boxed()
                }
            };
            self.wires.lock().unwrap().push(wire);
            Link {
                connection: near,
                exit,
            }
        });
        futures::future::ready(link).boxed()
    }
}

/// A server answering [`Echo`] with `answer`.
fn server(answer: fn(String) -> BoxFuture<'static, String>) -> Server {
    let (server, runner) = Server::new();
    tokio::spawn(runner);
    server.register::<Echo, _, _>(move |text| answer(text).map(Ok));
    server
}

fn echo(text: String) -> BoxFuture<'static, String> {
    futures::future::ready(text).boxed()
}

fn never(_: String) -> BoxFuture<'static, String> {
    futures::future::pending().boxed()
}

/// Runs a session over `mock`, with its statuses from before it started.
fn start(mock: Mock) -> (Arc<Session<Mock>>, Statuses, JoinHandle<anyhow::Error>) {
    start_with(Client::new().0, mock)
}

/// [`start`], attaching `client`.
fn start_with(
    client: Client,
    mock: Mock,
) -> (Arc<Session<Mock>>, Statuses, JoinHandle<anyhow::Error>) {
    let session = Arc::new(Session::with_heartbeat(client, mock, HEARTBEAT));
    let statuses = session.statuses();
    let running = tokio::spawn({
        let session = session.clone();
        async move {
            let Err(error) = session.run().await;
            error
        }
    });
    (session, statuses, running)
}

/// The statuses up to and including `status`.
async fn until(statuses: &mut Statuses, status: Status) -> Vec<Status> {
    let mut seen = Vec::new();
    tokio::time::timeout(WAIT, async {
        while let Some(next) = statuses.next().await {
            seen.push(next);
            if next == status {
                return;
            }
        }
    })
    .await
    .unwrap_or_else(|_| panic!("no {status:?} after {seen:?}"));
    seen
}

#[tokio::test(flavor = "multi_thread")]
async fn a_severed_wire_reconnects_and_resends() {
    let server = server(echo);
    let (session, mut statuses, _running) = start(Mock::new([
        Plan::Serve(server.clone()),
        Plan::Serve(server),
    ]));
    assert_eq!(
        until(&mut statuses, Status::Connected).await,
        [Status::Disconnected, Status::Connecting, Status::Connected]
    );

    let echo = session.client().service::<Echo>();
    session.transport().sever();
    let answer = tokio::spawn(async move { echo.request(&"across".to_owned()).await });
    assert_eq!(
        until(&mut statuses, Status::Connected).await,
        [Status::Reconnecting, Status::Connected]
    );
    let answer = tokio::time::timeout(WAIT, answer).await.unwrap();
    assert_eq!(answer.unwrap().unwrap(), "across");
    assert_eq!(session.transport().modes(), [Mode::Start, Mode::Reconnect]);
}

#[tokio::test(flavor = "multi_thread")]
async fn missed_heartbeats_reconnect_and_resend() {
    let (session, mut statuses, _running) =
        start(Mock::new([Plan::Stall, Plan::Serve(server(echo))]));
    let echo = session.client().service::<Echo>();
    let answer = tokio::spawn(async move { echo.request(&"waited".to_owned()).await });

    assert_eq!(
        until(&mut statuses, Status::Connected).await,
        [
            Status::Disconnected,
            Status::Connecting,
            Status::Reconnecting,
            Status::Connected
        ]
    );
    let answer = tokio::time::timeout(WAIT, answer).await.unwrap();
    assert_eq!(answer.unwrap().unwrap(), "waited");
    assert_eq!(session.transport().modes(), [Mode::Start, Mode::Reconnect]);
}

#[tokio::test(flavor = "multi_thread")]
async fn no_daemon_starts_a_fresh_one_and_fails_what_waited_on_the_old() {
    let (session, mut statuses, _running) = start(Mock::new([
        Plan::Serve(server(never)),
        Plan::NoDaemon,
        Plan::Serve(server(echo)),
    ]));
    until(&mut statuses, Status::Connected).await;

    let echo = session.client().service::<Echo>();
    let waiting = tokio::spawn({
        let echo = echo.clone();
        async move { echo.request(&"lost".to_owned()).await }
    });
    tokio::time::sleep(Duration::from_millis(50)).await;
    session.transport().sever();

    assert_eq!(
        until(&mut statuses, Status::Connected).await,
        [Status::Reconnecting, Status::Connected]
    );
    let waiting = tokio::time::timeout(WAIT, waiting).await.unwrap();
    assert!(matches!(waiting.unwrap(), Err(Error::Restarted)));
    assert_eq!(echo.request(&"fresh".to_owned()).await.unwrap(), "fresh");
    assert_eq!(
        session.transport().modes(),
        [Mode::Start, Mode::Reconnect, Mode::Start]
    );
}

#[cfg(unix)]
#[tokio::test(flavor = "multi_thread")]
async fn no_daemon_ends_the_processes_of_the_old_one() {
    use cydonia_cupola::exec::{Command, Exec, Output};

    fn exec_server() -> Server {
        let (server, runner) = Server::new();
        tokio::spawn(runner);
        Exec::register(&server);
        server
    }
    fn shell(script: &str) -> Command {
        let mut command = Command::new("/bin/sh");
        command.args = vec!["-c".into(), script.into()];
        command
    }

    let (session, mut statuses, _running) = start(Mock::new([
        Plan::Serve(exec_server()),
        Plan::NoDaemon,
        Plan::Serve(exec_server()),
    ]));
    until(&mut statuses, Status::Connected).await;
    let exec = session.client().service::<Exec>();
    let mut old = exec.start(&shell("sleep 30")).await.unwrap();

    session.transport().sever();
    assert_eq!(
        until(&mut statuses, Status::Connected).await,
        [Status::Reconnecting, Status::Connected]
    );
    let ended = tokio::time::timeout(WAIT, old.next()).await.unwrap();
    assert!(matches!(ended, Some(Err(Error::Restarted))), "{ended:?}");
    assert!(old.next().await.is_none());

    let mut fresh = exec.start(&shell("read line; echo fresh")).await.unwrap();
    assert_eq!(fresh.id(), old.id());
    assert!(matches!(old.kill().await, Err(Error::Restarted)));
    assert!(matches!(old.write(b"x").await, Err(Error::Restarted)));
    assert!(matches!(old.close_stdin().await, Err(Error::Restarted)));
    fresh.write(b"\n").await.unwrap();
    let mut outputs = Vec::new();
    while let Some(output) = tokio::time::timeout(WAIT, fresh.next()).await.unwrap() {
        outputs.push(output.unwrap());
    }
    assert_eq!(
        outputs,
        [Output::Stdout(b"fresh\n".to_vec()), Output::Exit(Some(0))]
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_session_gives_up_after_three_failed_opens() {
    let (session, mut statuses, running) = start(Mock::new([Plan::Serve(server(echo))]));
    until(&mut statuses, Status::Connected).await;

    session.transport().sever();
    let error = tokio::time::timeout(WAIT, running).await.unwrap().unwrap();
    assert_eq!(error.to_string(), "unreachable");
    assert_eq!(
        until(&mut statuses, Status::Disconnected).await,
        [Status::Reconnecting, Status::Disconnected]
    );
    assert_eq!(
        session.transport().modes(),
        [
            Mode::Start,
            Mode::Reconnect,
            Mode::Reconnect,
            Mode::Reconnect
        ]
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_failed_first_open_ends_the_session() {
    let (session, mut statuses, running) = start(Mock::new([]));
    let error = tokio::time::timeout(WAIT, running).await.unwrap().unwrap();
    assert_eq!(error.to_string(), "unreachable");
    assert_eq!(statuses.next().await, Some(Status::Disconnected));
    assert_eq!(
        until(&mut statuses, Status::Disconnected).await,
        [Status::Connecting, Status::Disconnected]
    );
    assert_eq!(session.status(), Status::Disconnected);
}

#[tokio::test(flavor = "multi_thread")]
async fn an_answer_for_the_old_server_is_never_sent_to_a_fresh_one() {
    let (old_entered, entered) = oneshot::channel::<()>();
    let (open_old, old_gate) = oneshot::channel::<String>();
    let (open_new, new_gate) = oneshot::channel::<String>();
    let gates = Arc::new(Mutex::new(VecDeque::from([
        (Some(old_entered), old_gate),
        (None, new_gate),
    ])));
    let (client, runner) = Client::new();
    tokio::spawn(runner);
    client.register::<Gated, _, _>(move |()| {
        let (entered, gate) = gates.lock().unwrap().pop_front().unwrap();
        async move {
            if let Some(entered) = entered {
                entered.send(()).unwrap();
            }
            Ok(gate.await?)
        }
    });

    let old = server(echo);
    let new = server(echo);
    let asked_old = old.service::<Gated>().send(&());
    let asked_new = tokio::spawn(new.service::<Gated>().send(&()));
    let (session, mut statuses, _running) = start_with(
        client,
        Mock::new([Plan::Serve(old), Plan::NoDaemon, Plan::Serve(new)]),
    );
    until(&mut statuses, Status::Connected).await;
    tokio::time::timeout(WAIT, entered).await.unwrap().unwrap();

    session.transport().sever();
    assert_eq!(
        until(&mut statuses, Status::Connected).await,
        [Status::Reconnecting, Status::Connected]
    );
    open_old.send("old".to_owned()).unwrap();
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert!(!asked_new.is_finished());
    open_new.send("new".to_owned()).unwrap();
    let answer = tokio::time::timeout(WAIT, asked_new).await.unwrap();
    assert_eq!(answer.unwrap().unwrap(), "new");
    drop(asked_old);
}
