use std::{
    future::Future,
    pin::Pin,
    task::{Context, Poll},
};

use futures::{
    FutureExt, StreamExt,
    channel::mpsc::{self, UnboundedReceiver, UnboundedSender},
    future::BoxFuture,
    stream::FuturesUnordered,
};

/// Runs the futures its [`Spawner`]s queue: the handlers of a
/// [`Server`](crate::Server) or a [`Client`](crate::Client), or a
/// [`Controller`](crate::client::controller::Controller)'s sessions. Completes once every [`Spawner`] is dropped and the futures
/// running then have finished.
pub struct Runner {
    jobs: UnboundedReceiver<BoxFuture<'static, ()>>,
    running: FuturesUnordered<BoxFuture<'static, ()>>,
    closed: bool,
}

/// Queues futures onto a [`Runner`].
#[derive(Clone)]
pub struct Spawner {
    jobs: UnboundedSender<BoxFuture<'static, ()>>,
}

impl Runner {
    pub(crate) fn new() -> (Spawner, Self) {
        let (jobs, queue) = mpsc::unbounded();
        let runner = Self {
            jobs: queue,
            running: FuturesUnordered::new(),
            closed: false,
        };
        (Spawner { jobs }, runner)
    }
}

impl Spawner {
    /// Runs `job` on the [`Runner`]. A job queued after the runner completed
    /// never runs.
    pub fn spawn(&self, job: impl Future<Output = ()> + Send + 'static) {
        let _ = self.jobs.unbounded_send(job.boxed());
    }
}

impl Future for Runner {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        while !self.closed {
            match self.jobs.poll_next_unpin(cx) {
                Poll::Ready(Some(job)) => self.running.push(job),
                Poll::Ready(None) => self.closed = true,
                Poll::Pending => break,
            }
        }
        loop {
            match self.running.poll_next_unpin(cx) {
                Poll::Ready(Some(())) => {}
                Poll::Ready(None) if self.closed => return Poll::Ready(()),
                Poll::Ready(None) | Poll::Pending => return Poll::Pending,
            }
        }
    }
}
