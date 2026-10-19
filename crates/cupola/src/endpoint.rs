//! One side of a connection, client or server alike: the services it
//! answers, its requests to the services the other side answers, and the
//! events each side pushes, all over one [`Peer`].

use std::{
    collections::HashMap,
    future::Future,
    marker::PhantomData,
    pin::Pin,
    sync::{Arc, Mutex, Weak},
    task::{Context, Poll},
};

use futures::{
    FutureExt, Stream, StreamExt,
    channel::{
        mpsc::{self, UnboundedReceiver, UnboundedSender},
        oneshot,
    },
    future::BoxFuture,
};

use crate::{
    Connection, Error, Runner, Service, Spawner,
    peer::Peer,
    wire::{Envelope, Fault, Kind},
};

type Handler = Arc<dyn Fn(Vec<u8>) -> BoxFuture<'static, Result<Vec<u8>, Fault>> + Send + Sync>;

/// A response payload, or why there is none.
enum Answer {
    Payload(Vec<u8>),
    Fault(Fault),
    Restarted,
}

/// What a subscription to a service's events is handed.
enum Pushed {
    Event(Vec<u8>),
    Restarted,
}

/// Which peer, of those started afresh one after another, a request was
/// exchanged with. A response answered under one epoch came from that peer,
/// and an answer is sent only within the epoch its request arrived in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Epoch(u64);

/// Requests waiting for their answers, and the epoch requests are sent and
/// answered in.
#[derive(Default)]
struct Calls {
    epoch: u64,
    waiting: HashMap<u64, oneshot::Sender<Answer>>,
    /// Set by [`Endpoint::close`]; never unset.
    closed: bool,
}

/// Locks are taken in field order: `subscribers`, then `calls`, then the
/// peer's own.
pub(crate) struct Endpoint {
    subscribers: Mutex<HashMap<&'static str, Vec<UnboundedSender<Pushed>>>>,
    calls: Mutex<Calls>,
    peer: Peer,
    handlers: Mutex<HashMap<&'static str, Handler>>,
    spawner: Spawner,
}

/// Typed access to one service the other side answers.
pub struct Handle<S> {
    endpoint: Arc<Endpoint>,
    service: PhantomData<fn() -> S>,
}

/// Pushes one service's events to the other side. Holds its side weakly.
pub struct Emitter<S> {
    endpoint: Weak<Endpoint>,
    service: PhantomData<fn() -> S>,
}

/// The events the other side pushes for one service from the time
/// [`Handle::events`] was called. Yields [`Error::Restarted`] once each time
/// the server is started afresh, where the events of the one before end and
/// the new one's begin. Ends when the client is closed, or its side and every
/// handle from it are dropped.
pub struct Events<S> {
    receiver: UnboundedReceiver<Pushed>,
    service: PhantomData<fn() -> S>,
}

impl Endpoint {
    /// Handlers run on the runner; it completes once the endpoint and every
    /// [`Spawner`] from it are dropped.
    pub(crate) fn new() -> (Arc<Self>, Runner) {
        let (spawner, runner) = Runner::new();
        let endpoint = Self {
            subscribers: Mutex::default(),
            calls: Mutex::default(),
            peer: Peer::new(),
            handlers: Mutex::default(),
            spawner,
        };
        (Arc::new(endpoint), runner)
    }

    /// # Panics
    ///
    /// If a service named `S::NAME` is already registered.
    pub(crate) fn register<S, F, Fut>(&self, handler: F)
    where
        S: Service,
        F: Fn(S::Request) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = anyhow::Result<S::Response>> + Send + 'static,
    {
        let handler: Handler =
            Arc::new(
                move |payload: Vec<u8>| match postcard::from_bytes::<S::Request>(&payload) {
                    Ok(request) => {
                        let response = handler(request);
                        async move {
                            let response = response
                                .await
                                .map_err(|error| Fault::Failed(format!("{error:#}")))?;
                            postcard::to_allocvec(&response)
                                .map_err(|error| Fault::Failed(format!("encode response: {error}")))
                        }
                        .boxed()
                    }
                    Err(error) => futures::future::ready(Err(Fault::Failed(format!(
                        "decode request: {error}"
                    ))))
                    .boxed(),
                },
            );
        let mut handlers = self.handlers.lock().unwrap();
        assert!(
            handlers.insert(S::NAME, handler).is_none(),
            "service {} registered twice",
            S::NAME
        );
    }

    pub(crate) fn handle<S: Service>(self: &Arc<Self>) -> Handle<S> {
        Handle {
            endpoint: self.clone(),
            service: PhantomData,
        }
    }

    pub(crate) fn emitter<S: Service>(self: &Arc<Self>) -> Emitter<S> {
        Emitter {
            endpoint: Arc::downgrade(self),
            service: PhantomData,
        }
    }

    pub(crate) fn spawner(&self) -> Spawner {
        self.spawner.clone()
    }

    /// Exchanges envelopes over `connection` until it ends.
    pub(crate) async fn attach(self: &Arc<Self>, connection: Connection) -> Result<(), Error> {
        self.peer
            .attach(connection, |envelope| self.receive(envelope))
            .await
    }

    /// Readies the side for a peer started afresh: every request waiting on
    /// the one before fails with [`Error::Restarted`], every [`Events`]
    /// yields it, nothing is resent to the new one, requests sent within the
    /// epoch before fail with it unsent, and the answers to requests that
    /// arrived within it are never sent. Called while no connection is
    /// attached.
    pub(crate) fn reset(&self) {
        let mut subscribers = self.subscribers.lock().unwrap();
        let waiting = {
            let mut calls = self.calls.lock().unwrap();
            self.peer.reset();
            calls.epoch += 1;
            std::mem::take(&mut calls.waiting)
        };
        for (_, sender) in waiting {
            let _ = sender.send(Answer::Restarted);
        }
        for senders in subscribers.values_mut() {
            senders.retain(|sender| sender.unbounded_send(Pushed::Restarted).is_ok());
        }
    }

    /// Ends the side for good: every request waiting and every one made
    /// after fails with [`Error::Closed`], unsent, every [`Events`] ends,
    /// and no request is answered.
    pub(crate) fn close(&self) {
        let mut subscribers = self.subscribers.lock().unwrap();
        let waiting = {
            let mut calls = self.calls.lock().unwrap();
            calls.closed = true;
            self.peer.reset();
            std::mem::take(&mut calls.waiting)
        };
        drop(waiting);
        subscribers.clear();
    }

    fn receive(self: &Arc<Self>, envelope: Envelope) {
        match envelope.kind {
            Kind::Request => self.serve(envelope),
            Kind::Response { to } => self.answer(to, Answer::Payload(envelope.payload)),
            Kind::Failure { to } => {
                let fault = postcard::from_bytes(&envelope.payload)
                    .unwrap_or_else(|error| Fault::Failed(format!("decode failure: {error}")));
                self.answer(to, Answer::Fault(fault));
            }
            Kind::Event => {
                let mut subscribers = self.subscribers.lock().unwrap();
                if let Some(senders) = subscribers.get_mut(envelope.service.as_str()) {
                    senders.retain(|sender| {
                        sender
                            .unbounded_send(Pushed::Event(envelope.payload.clone()))
                            .is_ok()
                    });
                }
            }
            Kind::Ack => {}
        }
    }

    /// Runs the request's handler on the runner and sends its answer.
    fn serve(self: &Arc<Self>, envelope: Envelope) {
        let epoch = {
            let calls = self.calls.lock().unwrap();
            if calls.closed {
                return;
            }
            Epoch(calls.epoch)
        };
        let Envelope {
            id,
            service,
            payload,
            ..
        } = envelope;
        let handler = self.handlers.lock().unwrap().get(service.as_str()).cloned();
        let Some(handler) = handler else {
            self.reply(epoch, &service, id, Err(Fault::NoService));
            return;
        };
        let answer = handler(payload);
        let endpoint = Arc::downgrade(self);
        self.spawner.spawn(async move {
            let answer = answer.await;
            if let Some(endpoint) = endpoint.upgrade() {
                endpoint.reply(epoch, &service, id, answer);
            }
        });
    }

    /// Sends the answer to request `to`, unless the side was reset or closed
    /// since `epoch`. A response that cannot be sent is answered with a
    /// [`Fault::Failed`] instead.
    fn reply(&self, epoch: Epoch, service: &str, to: u64, answer: Result<Vec<u8>, Fault>) {
        let calls = self.calls.lock().unwrap();
        if calls.closed || Epoch(calls.epoch) != epoch {
            return;
        }
        let fault = match answer {
            Ok(payload) => match self.peer.send(service, Kind::Response { to }, payload) {
                Ok(_) => return,
                Err(error) => Fault::Failed(error.to_string()),
            },
            Err(fault) => fault,
        };
        let payload = postcard::to_allocvec(&fault).expect("a fault encodes");
        let _ = self.peer.send(service, Kind::Failure { to }, payload);
    }

    fn answer(&self, to: u64, answer: Answer) {
        if let Some(sender) = self.calls.lock().unwrap().waiting.remove(&to) {
            let _ = sender.send(answer);
        }
    }
}

impl<S: Service> Handle<S> {
    /// Sends `request` and waits for the answer, across reattached
    /// connections.
    pub async fn request(&self, request: &S::Request) -> Result<S::Response, Error> {
        self.send(request).await
    }

    /// [`Handle::request`], with the request sent at the call rather than
    /// when the answer is first polled. Requests sent from one side reach the
    /// other in the order they were sent.
    pub fn send(
        &self,
        request: &S::Request,
    ) -> impl Future<Output = Result<S::Response, Error>> + Send + use<S> {
        let sent = self.sent(request, None);
        async move { Self::answered(sent?.0).await }
    }

    /// [`Handle::request`], answering the epoch it was sent in. With
    /// `within`, fails with [`Error::Restarted`], sending nothing, unless
    /// that epoch is still the current one.
    pub(crate) async fn request_within(
        &self,
        request: &S::Request,
        within: Option<Epoch>,
    ) -> Result<(S::Response, Epoch), Error> {
        let (receiver, epoch) = self.sent(request, within)?;
        Ok((Self::answered(receiver).await?, epoch))
    }

    fn sent(
        &self,
        request: &S::Request,
        within: Option<Epoch>,
    ) -> Result<(oneshot::Receiver<Answer>, Epoch), Error> {
        let payload = postcard::to_allocvec(request).map_err(Error::Encode)?;
        let (sender, receiver) = oneshot::channel();
        let mut calls = self.endpoint.calls.lock().unwrap();
        if calls.closed {
            return Err(Error::Closed);
        }
        let epoch = Epoch(calls.epoch);
        if within.is_some_and(|within| within != epoch) {
            return Err(Error::Restarted);
        }
        let id = self.endpoint.peer.send(S::NAME, Kind::Request, payload)?;
        calls.waiting.insert(id, sender);
        Ok((receiver, epoch))
    }

    async fn answered(receiver: oneshot::Receiver<Answer>) -> Result<S::Response, Error> {
        match receiver.await.map_err(|_| Error::Closed)? {
            Answer::Payload(payload) => postcard::from_bytes(&payload).map_err(Error::Decode),
            Answer::Fault(Fault::NoService) => Err(Error::NoService(S::NAME.to_owned())),
            Answer::Fault(Fault::Failed(message)) => Err(Error::Remote {
                service: S::NAME.to_owned(),
                message,
            }),
            Answer::Restarted => Err(Error::Restarted),
        }
    }

    pub fn events(&self) -> Events<S> {
        let (sender, receiver) = mpsc::unbounded();
        let mut subscribers = self.endpoint.subscribers.lock().unwrap();
        if !self.endpoint.calls.lock().unwrap().closed {
            subscribers.entry(S::NAME).or_default().push(sender);
        }
        drop(subscribers);
        Events {
            receiver,
            service: PhantomData,
        }
    }
}

impl<S> Clone for Handle<S> {
    fn clone(&self) -> Self {
        Self {
            endpoint: self.endpoint.clone(),
            service: PhantomData,
        }
    }
}

impl<S: Service> Emitter<S> {
    /// Queues `event` for the other side, written now if a connection is
    /// attached and resent like any envelope.
    pub fn emit(&self, event: &S::Event) -> Result<(), Error> {
        let endpoint = self.endpoint.upgrade().ok_or(Error::Closed)?;
        let payload = postcard::to_allocvec(event).map_err(Error::Encode)?;
        endpoint.peer.send(S::NAME, Kind::Event, payload)?;
        Ok(())
    }
}

impl<S> Clone for Emitter<S> {
    fn clone(&self) -> Self {
        Self {
            endpoint: self.endpoint.clone(),
            service: PhantomData,
        }
    }
}

impl<S: Service> Stream for Events<S> {
    type Item = Result<S::Event, Error>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.receiver.poll_next_unpin(cx).map(|pushed| {
            pushed.map(|pushed| match pushed {
                Pushed::Event(payload) => postcard::from_bytes(&payload).map_err(Error::Decode),
                Pushed::Restarted => Err(Error::Restarted),
            })
        })
    }
}
