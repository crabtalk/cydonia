use std::{future::Future, sync::Arc};

use crate::{
    Connection, Emitter, Error, Handle, Runner, Service, Spawner, endpoint::Endpoint, ping::Ping,
};

/// The side a [`Client`](crate::Client) connects to; on Unix, a
/// [`Daemon`](crate::daemon::Daemon) runs it.
#[derive(Clone)]
pub struct Server {
    endpoint: Arc<Endpoint>,
}

impl Server {
    /// A server with [`Ping`] registered. Requests are answered only while
    /// the runner is polled; it completes once every [`Server`] clone, every
    /// [`Handle`] from them and every [`Spawner`] from them is dropped.
    pub fn new() -> (Self, Runner) {
        let (endpoint, runner) = Endpoint::new();
        let server = Self { endpoint };
        Ping::register(&server);
        (server, runner)
    }

    /// Answers the client's requests for `S` with `handler`. An `Err`
    /// reaches the client as [`Error::Remote`] carrying its `{:#}` text. A
    /// [`Server`] or [`Handle`] that `handler` holds keeps the server alive.
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

    /// The service `S` the client registered. A request made while no
    /// client is attached goes out once one is.
    pub fn service<S: Service>(&self) -> Handle<S> {
        self.endpoint.handle()
    }

    pub fn emitter<S: Service>(&self) -> Emitter<S> {
        self.endpoint.emitter()
    }

    /// Exchanges envelopes over `connection` until it ends.
    pub async fn attach(&self, connection: Connection) -> Result<(), Error> {
        self.endpoint.attach(connection).await
    }

    /// Does not keep the server alive.
    pub fn spawner(&self) -> Spawner {
        self.endpoint.spawner()
    }
}
