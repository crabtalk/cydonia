//! The `ping` service: a request answered with nothing, by every [`Server`]
//! from [`Server::new`] on. A client's heartbeat sends it.

use crate::{Server, Service};

pub struct Ping;

impl Service for Ping {
    const NAME: &'static str = "ping";
    type Request = ();
    type Response = ();
    type Event = ();
}

impl Ping {
    pub(crate) fn register(server: &Server) {
        server.register::<Self, _, _>(|()| async { Ok(()) });
    }
}
