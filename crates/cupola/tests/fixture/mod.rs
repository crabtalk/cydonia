//! A client and a server joined by a mock wire, each side's attachment on a
//! tokio task.

#![allow(dead_code)]

use cydonia_cupola::{Client, Server, mock::Wire};
use tokio::task::JoinHandle;

pub fn attach(client: &Client, server: &Server) -> (Wire, JoinHandle<()>, JoinHandle<()>) {
    let (wire, near, far) = Wire::pair();
    let client = client.clone();
    let server = server.clone();
    let client = tokio::spawn(async move {
        let _ = client.attach(near).await;
    });
    let server = tokio::spawn(async move {
        let _ = server.attach(far).await;
    });
    (wire, client, server)
}

/// A server with Cupola's own services, its runner on a tokio task.
pub fn builtin_server() -> Server {
    let (server, runner) = Server::new();
    tokio::spawn(runner);
    cydonia_cupola::fs::Fs::register(&server);
    cydonia_cupola::exec::Exec::register(&server);
    server
}
