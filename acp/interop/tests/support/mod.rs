//! Shared plumbing for the interop tests.

use agent_client_protocol as official;
use std::{
    future::Future,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio_util::compat::{TokioAsyncReadCompatExt, TokioAsyncWriteCompatExt};

/// Wraps a tokio stream as the byte transport the official SDK connects over.
pub fn byte_streams<S: AsyncRead + AsyncWrite + Send + 'static>(
    stream: S,
) -> official::ByteStreams<
    impl futures::AsyncWrite + Send + Unpin + 'static,
    impl futures::AsyncRead + Send + Unpin + 'static,
> {
    let (read, write) = tokio::io::split(stream);
    official::ByteStreams::new(write.compat_write(), read.compat())
}

/// Records which methods a side was actually asked to serve.
#[derive(Clone, Default)]
pub struct Served(Arc<Mutex<Vec<&'static str>>>);

impl Served {
    pub fn saw(&self, method: &'static str) {
        self.0.lock().expect("poisoned").push(method);
    }

    /// Every method in `expected` must have been served, in any order.
    #[track_caller]
    pub fn assert_saw(&self, expected: &[&'static str]) {
        let seen = self.0.lock().expect("poisoned").clone();
        let missing: Vec<_> = expected.iter().filter(|m| !seen.contains(m)).collect();
        assert!(
            missing.is_empty(),
            "never served: {missing:?} (saw {seen:?})"
        );
    }
}

/// Runs `work`, failing the test rather than hanging if a peer goes quiet.
pub async fn within<F: Future>(work: F) -> F::Output {
    tokio::time::timeout(Duration::from_secs(20), work)
        .await
        .expect("a peer stopped answering")
}
