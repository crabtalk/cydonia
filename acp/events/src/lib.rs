//! A channel-based [`Client`](cydonia_acp::Client) for [`cydonia_acp`].
//!
//! A frontend already has an event loop. [`channel`] hands it one instead of a
//! trait to implement: a [`Client`](cydonia_acp::Client) to give [`spawn`](cydonia_acp::spawn),
//! and the [`Events`] stream every call from the agent arrives on.
//!
//! Anything you do not match on is declined with `method not found`, exactly as
//! an unimplemented [`Client`](cydonia_acp::Client) method would be — so a consumer
//! that ignores an [`Event`] is indistinguishable on the wire from one that
//! never served that method. Declare what you do serve in
//! [`ClientCapabilities`](cydonia_acp_proto::ClientCapabilities), as ever.
//!
//! Notifications arrive in wire order. A request is dispatched on its own task,
//! so it can reach the stream just after a notification that followed it on the
//! wire.
//!
//! ```no_run
//! use cydonia_acp_events::Event;
//!
//! # async fn run() -> cydonia_acp::Result<()> {
//! let (client, mut events) = cydonia_acp_events::channel();
//! let (_agent, _child) = cydonia_acp::spawn(&mut tokio::process::Command::new("my-agent"), client, None)?;
//!
//! while let Some(event) = events.recv().await {
//!     match event {
//!         Event::Update(notification) => println!("{:?}", notification.update),
//!         Event::Permission(request, reply) => reply.send(
//!             cydonia_acp::proto::RequestPermissionResponse::selected(
//!                 request.options[0].option_id.clone(),
//!             ),
//!         ),
//!         _ => {}
//!     }
//! }
//! # Ok(()) }
//! ```

pub use event::Event;
pub use reply::Reply;
use std::sync::Arc;
use tokio::sync::{mpsc, oneshot};

mod client;
mod event;
mod reply;

/// Every call the agent makes, in one stream.
pub type Events = mpsc::UnboundedReceiver<Event>;

/// A [`Client`](cydonia_acp::Client) that forwards its calls to an [`Events`] stream.
pub struct Channel(mpsc::UnboundedSender<Event>);

/// A client to hand to [`spawn`](cydonia_acp::spawn), and the stream its calls arrive
/// on.
pub fn channel() -> (Arc<Channel>, Events) {
    // Unbounded: `session_update` is awaited by the read loop, so backpressure
    // here would stall the responses the consumer's own calls are waiting on.
    let (tx, rx) = mpsc::unbounded_channel();
    (Arc::new(Channel(tx)), rx)
}

impl Channel {
    /// Queue a request and wait on the consumer. A dropped [`Reply`] — or a
    /// dropped [`Events`] — declines it.
    pub(crate) async fn ask<T>(
        &self,
        event: impl FnOnce(Reply<T>) -> Event,
    ) -> cydonia_acp_proto::Result<T> {
        let (tx, rx) = oneshot::channel();
        self.0
            .send(event(Reply(tx)))
            .map_err(|_| cydonia_acp_proto::Error::method_not_found())?;
        rx.await
            .unwrap_or_else(|_| Err(cydonia_acp_proto::Error::method_not_found()))
    }

    pub(crate) fn tell(&self, event: Event) {
        let _ = self.0.send(event);
    }
}
