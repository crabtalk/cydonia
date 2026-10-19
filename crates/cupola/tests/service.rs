mod fixture;

use std::sync::{
    Arc, Mutex,
    atomic::{AtomicUsize, Ordering},
};

use cydonia_cupola::{Client, Error, Server, Service, wire::MAX_FRAME};
use fixture::attach;
use futures::{StreamExt, channel::oneshot};
use serde::{Deserialize, Serialize};

struct Echo;

#[derive(Debug, PartialEq, Serialize, Deserialize)]
enum Ask {
    Echo(String),
    Fail,
    /// Answered with this many bytes of text.
    Fill(usize),
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
struct Tick(u32);

impl Service for Echo {
    const NAME: &'static str = "echo";
    type Request = Ask;
    type Response = String;
    type Event = Tick;
}

struct Unregistered;

impl Service for Unregistered {
    const NAME: &'static str = "unregistered";
    type Request = ();
    type Response = ();
    type Event = ();
}

fn echo_server() -> Server {
    let (server, runner) = Server::new();
    tokio::spawn(runner);
    server.register::<Echo, _, _>(|ask| async move {
        match ask {
            Ask::Echo(text) => Ok(text),
            Ask::Fail => anyhow::bail!("asked to fail"),
            Ask::Fill(len) => Ok("x".repeat(len)),
        }
    });
    server
}

#[tokio::test]
async fn a_request_gets_its_response() {
    let server = echo_server();
    let (client, _) = Client::new();
    let _link = attach(&client, &server);

    let echo = client.service::<Echo>();
    let answer = echo.request(&Ask::Echo("hello".into())).await.unwrap();
    assert_eq!(answer, "hello");
}

#[tokio::test]
async fn concurrent_requests_get_their_own_responses() {
    let server = echo_server();
    let (client, _) = Client::new();
    let _link = attach(&client, &server);

    let echo = client.service::<Echo>();
    let answers = futures::future::join_all((0..50).map(|n| {
        let echo = echo.clone();
        async move { echo.request(&Ask::Echo(n.to_string())).await.unwrap() }
    }))
    .await;
    let expected: Vec<String> = (0..50).map(|n| n.to_string()).collect();
    assert_eq!(answers, expected);
}

#[tokio::test]
async fn a_handler_error_reaches_the_client() {
    let server = echo_server();
    let (client, _) = Client::new();
    let _link = attach(&client, &server);

    let error = client
        .service::<Echo>()
        .request(&Ask::Fail)
        .await
        .unwrap_err();
    match error {
        Error::Remote { service, message } => {
            assert_eq!(service, "echo");
            assert_eq!(message, "asked to fail");
        }
        other => panic!("expected a remote error, got {other:?}"),
    }
}

#[tokio::test]
async fn a_service_the_server_lacks_is_named() {
    let server = echo_server();
    let (client, _) = Client::new();
    let _link = attach(&client, &server);

    let error = client
        .service::<Unregistered>()
        .request(&())
        .await
        .unwrap_err();
    assert!(matches!(error, Error::NoService(name) if name == "unregistered"));
}

#[tokio::test]
async fn events_reach_every_subscriber() {
    let server = echo_server();
    let (client, _) = Client::new();
    let _link = attach(&client, &server);

    let echo = client.service::<Echo>();
    let mut first = echo.events();
    let mut second = echo.events();
    let emitter = server.emitter::<Echo>();
    emitter.emit(&Tick(1)).unwrap();
    emitter.emit(&Tick(2)).unwrap();

    for events in [&mut first, &mut second] {
        assert_eq!(events.next().await.unwrap().unwrap(), Tick(1));
        assert_eq!(events.next().await.unwrap().unwrap(), Tick(2));
    }
}

#[tokio::test]
async fn a_request_made_while_detached_goes_out_on_attach() {
    let server = echo_server();
    let (client, _) = Client::new();

    let echo = client.service::<Echo>();
    let answer = tokio::spawn(async move { echo.request(&Ask::Echo("later".into())).await });
    let mut events = client.service::<Echo>().events();
    server.emitter::<Echo>().emit(&Tick(7)).unwrap();

    let _link = attach(&client, &server);
    assert_eq!(answer.await.unwrap().unwrap(), "later");
    assert_eq!(events.next().await.unwrap().unwrap(), Tick(7));
}

#[tokio::test]
async fn a_response_lost_to_a_cut_link_is_resent_once_on_reattach() {
    let (server, runner) = Server::new();
    tokio::spawn(runner);
    let calls = Arc::new(AtomicUsize::new(0));
    let (entered, handler_entered) = oneshot::channel::<()>();
    let (release, released) = oneshot::channel::<()>();
    let gate = Arc::new(Mutex::new(Some((entered, released))));
    server.register::<Echo, _, _>({
        let calls = calls.clone();
        move |ask| {
            calls.fetch_add(1, Ordering::SeqCst);
            let gate = gate.lock().unwrap().take();
            async move {
                if let Some((entered, released)) = gate {
                    entered.send(()).unwrap();
                    released.await.unwrap();
                }
                match ask {
                    Ask::Echo(text) => Ok(text),
                    other => anyhow::bail!("not asked of this server: {other:?}"),
                }
            }
        }
    });
    let (client, _) = Client::new();
    let (wire, client_side, server_side) = attach(&client, &server);

    let echo = client.service::<Echo>();
    let answer = tokio::spawn(async move { echo.request(&Ask::Echo("survives".into())).await });
    handler_entered.await.unwrap();
    wire.sever();
    client_side.await.unwrap();
    server_side.await.unwrap();
    release.send(()).unwrap();

    let _link = attach(&client, &server);
    assert_eq!(answer.await.unwrap().unwrap(), "survives");
    let after = client
        .service::<Echo>()
        .request(&Ask::Echo("next".into()))
        .await
        .unwrap();
    assert_eq!(after, "next");
    assert_eq!(calls.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn a_request_over_the_frame_limit_is_refused_and_the_link_carries_on() {
    let server = echo_server();
    let (client, _) = Client::new();
    let _link = attach(&client, &server);

    let echo = client.service::<Echo>();
    let huge = "x".repeat(MAX_FRAME as usize);
    let error = echo.request(&Ask::Echo(huge)).await.unwrap_err();
    assert!(matches!(error, Error::Oversized(_)), "{error:?}");
    let answer = echo.request(&Ask::Echo("after".into())).await.unwrap();
    assert_eq!(answer, "after");
}

#[tokio::test]
async fn a_response_over_the_frame_limit_arrives_as_a_failure() {
    let server = echo_server();
    let (client, _) = Client::new();
    let _link = attach(&client, &server);

    let echo = client.service::<Echo>();
    let error = echo
        .request(&Ask::Fill(MAX_FRAME as usize))
        .await
        .unwrap_err();
    assert!(matches!(error, Error::Remote { .. }), "{error:?}");
    let answer = echo.request(&Ask::Echo("after".into())).await.unwrap();
    assert_eq!(answer, "after");
}
