//! Services a client registers, called by the server and by the server's own
//! handlers.

mod fixture;

use cydonia_cupola::{Client, Error, Server, Service};
use fixture::attach;
use futures::StreamExt as _;
use serde::{Deserialize, Serialize};

struct Shout;

#[derive(Debug, PartialEq, Serialize, Deserialize)]
enum Ask {
    Shout(String),
    Fail,
}

impl Service for Shout {
    const NAME: &'static str = "shout";
    type Request = Ask;
    type Response = String;
    type Event = String;
}

/// Answered by the server, by asking the client's [`Shout`].
struct Relay;

impl Service for Relay {
    const NAME: &'static str = "relay";
    type Request = String;
    type Response = String;
    type Event = ();
}

struct Unregistered;

impl Service for Unregistered {
    const NAME: &'static str = "unregistered";
    type Request = ();
    type Response = ();
    type Event = ();
}

fn server() -> Server {
    let (server, runner) = Server::new();
    tokio::spawn(runner);
    server
}

fn shouting_client() -> Client {
    let (client, runner) = Client::new();
    tokio::spawn(runner);
    client.register::<Shout, _, _>(|ask| async move {
        match ask {
            Ask::Shout(text) => Ok(text.to_uppercase()),
            Ask::Fail => anyhow::bail!("asked to fail"),
        }
    });
    client
}

#[tokio::test]
async fn the_server_calls_a_service_the_client_registered() {
    let server = server();
    let client = shouting_client();
    let _link = attach(&client, &server);

    let shout = server.service::<Shout>();
    let answer = shout.request(&Ask::Shout("hello".into())).await.unwrap();
    assert_eq!(answer, "HELLO");
}

#[tokio::test]
async fn a_client_handler_error_reaches_the_server() {
    let server = server();
    let client = shouting_client();
    let _link = attach(&client, &server);

    let error = server
        .service::<Shout>()
        .request(&Ask::Fail)
        .await
        .unwrap_err();
    match error {
        Error::Remote { service, message } => {
            assert_eq!(service, "shout");
            assert_eq!(message, "asked to fail");
        }
        other => panic!("expected a remote error, got {other:?}"),
    }
}

#[tokio::test]
async fn a_service_the_client_lacks_is_named() {
    let server = server();
    let client = shouting_client();
    let _link = attach(&client, &server);

    let error = server
        .service::<Unregistered>()
        .request(&())
        .await
        .unwrap_err();
    assert!(matches!(error, Error::NoService(name) if name == "unregistered"));
}

#[tokio::test]
async fn the_server_receives_the_client_events() {
    let server = server();
    let client = shouting_client();
    let _link = attach(&client, &server);

    let mut events = server.service::<Shout>().events();
    client.emitter::<Shout>().emit(&"up".to_owned()).unwrap();
    assert_eq!(events.next().await.unwrap().unwrap(), "up");
}

#[tokio::test]
async fn a_call_made_while_the_client_is_detached_completes_after_reattach() {
    let server = server();
    let client = shouting_client();
    let (wire, client_side, server_side) = attach(&client, &server);
    wire.sever();
    client_side.await.unwrap();
    server_side.await.unwrap();

    let shout = server.service::<Shout>();
    let answer = tokio::spawn(async move { shout.request(&Ask::Shout("later".into())).await });
    let _link = attach(&client, &server);
    assert_eq!(answer.await.unwrap().unwrap(), "LATER");
}

#[tokio::test]
async fn a_server_handler_forwards_to_a_client_service() {
    let server = server();
    let shout = server.service::<Shout>();
    server.register::<Relay, _, _>(move |text| {
        let shout = shout.clone();
        async move { Ok(shout.request(&Ask::Shout(text)).await?) }
    });
    let client = shouting_client();
    let _link = attach(&client, &server);

    let answer = client
        .service::<Relay>()
        .request(&"through".to_owned())
        .await
        .unwrap();
    assert_eq!(answer, "THROUGH");
}
