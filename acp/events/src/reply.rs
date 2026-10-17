//! The answer half of a request that arrived as an [`Event`](crate::Event).

use std::fmt;
use tokio::sync::oneshot;

/// A request waiting on the consumer.
///
/// Dropping it declines the request with `method not found` — the same refusal
/// [`Client`](cydonia_acp::Client) sends for a method left unimplemented.
pub struct Reply<T>(pub(crate) oneshot::Sender<cydonia_acp_proto::Result<T>>);

impl<T> Reply<T> {
    pub fn send(self, value: T) {
        let _ = self.0.send(Ok(value));
    }

    pub fn fail(self, error: cydonia_acp_proto::Error) {
        let _ = self.0.send(Err(error));
    }

    /// True once the agent has cancelled the request; a prompt still on screen
    /// for it should come down.
    pub fn is_cancelled(&self) -> bool {
        self.0.is_closed()
    }

    /// Resolves when the agent cancels the request.
    pub async fn cancelled(&mut self) {
        self.0.closed().await;
    }
}

impl<T> fmt::Debug for Reply<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Reply")
            .field("cancelled", &self.is_cancelled())
            .finish()
    }
}
