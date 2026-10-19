//! The client side: [`Client`] exchanges envelopes over whatever connection
//! it is attached to; a [`session::Session`] keeps it attached through a
//! [`session::Transport`], such as [`ssh::Remote`] on Unix, and a
//! [`controller::Controller`] holds one session per saved host.

#[cfg(unix)]
pub mod askpass;
#[cfg(unix)]
pub mod controller;
pub mod host;
pub mod platform;
pub mod session;
#[cfg(unix)]
pub mod ssh;

use std::{future::Future, sync::Arc};

use crate::{Connection, Emitter, Error, Handle, Runner, Service, endpoint::Endpoint};

/// The side that connects to a [`Server`](crate::Server).
#[derive(Clone)]
pub struct Client {
    endpoint: Arc<Endpoint>,
}

impl Client {
    /// Requests from the server are answered only while the runner is
    /// polled; it completes once every [`Client`] clone and every [`Handle`]
    /// from them is dropped.
    pub fn new() -> (Self, Runner) {
        let (endpoint, runner) = Endpoint::new();
        (Self { endpoint }, runner)
    }

    /// The service `S` the server registered.
    pub fn service<S: Service>(&self) -> Handle<S> {
        self.endpoint.handle()
    }

    /// Answers the server's requests for `S` with `handler`. An `Err`
    /// reaches the server as [`Error::Remote`] carrying its `{:#}` text. A
    /// [`Client`] or [`Handle`] that `handler` holds keeps the client alive.
    ///
    /// # Panics
    ///
    /// If a service named `S::NAME` is already registered.
    pub fn register<S, F, Fut>(&self, handler: F)
    where
        S: Service,
        F: Fn(S::Request) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = anyhow::Result<S::Response>> + Send + 'static,
    {
        self.endpoint.register::<S, F, Fut>(handler);
    }

    pub fn emitter<S: Service>(&self) -> Emitter<S> {
        self.endpoint.emitter()
    }

    /// Exchanges envelopes over `connection` until it ends.
    pub async fn attach(&self, connection: Connection) -> Result<(), Error> {
        self.endpoint.attach(connection).await
    }

    /// Whether `other` is this client or a clone of it.
    pub fn same(&self, other: &Client) -> bool {
        Arc::ptr_eq(&self.endpoint, &other.endpoint)
    }

    /// [`Endpoint::reset`], for a server started afresh.
    pub(crate) fn reset(&self) {
        self.endpoint.reset();
    }

    /// [`Endpoint::close`].
    pub(crate) fn close(&self) {
        self.endpoint.close();
    }
}
