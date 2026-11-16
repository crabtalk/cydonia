//! The bidirectional JSON-RPC connection both roles are built on.

use crate::{
    codec::{self, Message, Tap},
    handler::Handler,
};
use serde::{Serialize, de::DeserializeOwned};
use std::{
    collections::HashMap,
    sync::{
        Arc, Mutex, Weak,
        atomic::{AtomicI64, Ordering},
    },
};
use tokio::{
    io::{AsyncBufRead, AsyncWrite, BufReader},
    sync::{mpsc, oneshot, watch},
    task::AbortHandle,
};

#[derive(Default)]
struct Calls {
    closed: bool,
    waiting: HashMap<
        cydonia_acp_proto::RequestId,
        oneshot::Sender<Result<serde_json::Value, cydonia_acp_proto::Error>>,
    >,
}

type Pending = Mutex<Calls>;
type InFlight = Arc<Mutex<HashMap<cydonia_acp_proto::RequestId, AbortHandle>>>;

/// A handle for calling the other side. Cheap to clone.
#[derive(Clone)]
pub struct Peer(Arc<Inner>);

struct Inner {
    outgoing: mpsc::UnboundedSender<Message>,
    pending: Pending,
    closed: watch::Sender<bool>,
    next_id: AtomicI64,
}

impl Peer {
    /// Wire a reader and a writer to `handler`, spawning the read and write
    /// loops. The returned [`Peer`] calls the other side. `tap` sees every
    /// line either loop moves.
    pub fn new<R, W, H>(reader: R, writer: W, handler: Arc<H>, tap: Option<Tap>) -> Self
    where
        R: AsyncBufRead + Unpin + Send + 'static,
        W: AsyncWrite + Unpin + Send + 'static,
        H: Handler,
    {
        let (outgoing, rx) = mpsc::unbounded_channel();
        let (closed, stopped) = watch::channel(false);
        let peer = Self(Arc::new(Inner {
            outgoing,
            pending: Pending::default(),
            closed,
            next_id: AtomicI64::new(0),
        }));
        tokio::spawn(write_loop(
            writer,
            rx,
            tap.clone(),
            Arc::downgrade(&peer.0),
            stopped,
        ));
        tokio::spawn(read_loop(reader, handler, peer.clone(), tap));
        peer
    }

    /// Wire up a duplex stream — a subprocess's stdio, a socket, an in-memory pipe.
    pub fn duplex<S, H>(stream: S, handler: Arc<H>, tap: Option<Tap>) -> Self
    where
        S: tokio::io::AsyncRead + AsyncWrite + Send + 'static,
        H: Handler,
    {
        let (reader, writer) = tokio::io::split(stream);
        Self::new(BufReader::new(reader), writer, handler, tap)
    }

    /// Call the other side and wait for its answer.
    pub async fn request<P: Serialize, R: DeserializeOwned>(
        &self,
        method: &str,
        params: P,
    ) -> Result<R, cydonia_acp_proto::Error> {
        let id = cydonia_acp_proto::RequestId::Num(self.0.next_id.fetch_add(1, Ordering::Relaxed));
        let params = serde_json::to_value(params)?;

        let (tx, rx) = oneshot::channel();
        {
            let mut calls = self.0.pending.lock().expect("pending map poisoned");
            if calls.closed {
                return Err(disconnected());
            }
            calls.waiting.insert(id.clone(), tx);
        }

        if self
            .0
            .outgoing
            .send(Message::request(id.clone(), method, params))
            .is_err()
        {
            self.0
                .pending
                .lock()
                .expect("pending map poisoned")
                .waiting
                .remove(&id);
            return Err(disconnected());
        }

        // Dropping this future before the answer arrives means the caller
        // stopped caring; tell the peer so it can stop working too.
        let abandon = Abandon {
            peer: self,
            id: id.clone(),
            armed: true,
        };
        let outcome = rx.await.map_err(|_| disconnected())?;
        abandon.disarm();
        Ok(serde_json::from_value(outcome?)?)
    }

    /// Tell the other side something. Returns once queued for writing.
    pub fn notify<P: Serialize>(
        &self,
        method: &str,
        params: P,
    ) -> Result<(), cydonia_acp_proto::Error> {
        let params = serde_json::to_value(params)?;
        if self.0.pending.lock().expect("pending map poisoned").closed {
            return Err(disconnected());
        }
        self.0
            .outgoing
            .send(Message::notification(method, params))
            .map_err(|_| disconnected())
    }

    fn resolve(
        &self,
        id: &cydonia_acp_proto::RequestId,
        outcome: Result<serde_json::Value, cydonia_acp_proto::Error>,
    ) {
        let waiter = self
            .0
            .pending
            .lock()
            .expect("pending map poisoned")
            .waiting
            .remove(id);
        if let Some(waiter) = waiter {
            let _ = waiter.send(outcome);
        }
    }

    /// Fail every outstanding call, so callers wake instead of hanging.
    fn fail_all(&self) {
        let pending = {
            let mut calls = self.0.pending.lock().expect("pending map poisoned");
            calls.closed = true;
            std::mem::take(&mut calls.waiting)
        };
        self.0.closed.send_replace(true);
        for (_, waiter) in pending {
            let _ = waiter.send(Err(disconnected()));
        }
    }

    fn send(&self, message: Message) {
        let _ = self.0.outgoing.send(message);
    }
}

/// Sends `$/cancel_request` for a call the caller walked away from.
struct Abandon<'a> {
    peer: &'a Peer,
    id: cydonia_acp_proto::RequestId,
    armed: bool,
}

impl Abandon<'_> {
    fn disarm(mut self) {
        self.armed = false;
    }
}

impl Drop for Abandon<'_> {
    fn drop(&mut self) {
        if !self.armed {
            return;
        }
        self.peer
            .0
            .pending
            .lock()
            .expect("pending map poisoned")
            .waiting
            .remove(&self.id);
        let _ = self.peer.notify(
            cydonia_acp_proto::method::CANCEL_REQUEST,
            cydonia_acp_proto::CancelRequestNotification {
                request_id: self.id.clone(),
                meta: None,
            },
        );
    }
}

fn disconnected() -> cydonia_acp_proto::Error {
    cydonia_acp_proto::Error::internal_error().data("connection closed")
}

async fn write_loop<W: AsyncWrite + Unpin>(
    mut writer: W,
    mut rx: mpsc::UnboundedReceiver<Message>,
    tap: Option<Tap>,
    peer: Weak<Inner>,
    mut closed: watch::Receiver<bool>,
) {
    loop {
        let message = tokio::select! {
            _ = closed.wait_for(|closed| *closed) => break,
            message = rx.recv() => match message { Some(message) => message, None => break },
        };
        let written = tokio::select! {
            _ = closed.wait_for(|closed| *closed) => break,
            written = codec::write(&mut writer, &message, tap.as_ref()) => written,
        };
        if written.is_err() {
            break;
        }
    }
    if let Some(inner) = peer.upgrade() {
        Peer(inner).fail_all();
    }
}

async fn read_loop<R, H>(mut reader: R, handler: Arc<H>, peer: Peer, tap: Option<Tap>)
where
    R: AsyncBufRead + Unpin,
    H: Handler,
{
    let in_flight = InFlight::default();
    let mut closed = peer.0.closed.subscribe();
    loop {
        let read = tokio::select! {
            _ = closed.wait_for(|closed| *closed) => break,
            read = codec::read(&mut reader, tap.as_ref()) => read,
        };
        let message = match read {
            Ok(Some(message)) => message,
            Ok(None) => break,
            // A malformed frame is the peer's bug, not ours; report and continue.
            // JSON-RPC answers an unparseable frame with a null id.
            Err(e) => {
                peer.send(Message::error(Some(cydonia_acp_proto::RequestId::Null), e));
                continue;
            }
        };
        match classify(message) {
            Frame::Response { id, outcome } => peer.resolve(&id, outcome),
            Frame::Notification { method, params } => {
                if method == cydonia_acp_proto::method::CANCEL_REQUEST {
                    cancel(&in_flight, &params);
                } else {
                    handler.notification(method, params).await;
                }
            }
            // Spawned, not awaited: a handler that blocks on a user prompt
            // must not stall the notification stream behind it. Responses
            // carry their id, so finishing out of order is correct.
            Frame::Request { id, method, params } => {
                let handler = Arc::clone(&handler);
                let peer = peer.clone();
                let done = Arc::clone(&in_flight);
                let task = tokio::spawn({
                    let id = id.clone();
                    async move {
                        let message = match handler.request(method, params).await {
                            Ok(result) => Message::response(id.clone(), result),
                            Err(e) => Message::error(Some(id.clone()), e),
                        };
                        done.lock().expect("in-flight map poisoned").remove(&id);
                        peer.send(message);
                    }
                });
                in_flight
                    .lock()
                    .expect("in-flight map poisoned")
                    .insert(id, task.abort_handle());
            }
            Frame::Ignore => {}
            Frame::Invalid => peer.send(Message::error(
                Some(cydonia_acp_proto::RequestId::Null),
                cydonia_acp_proto::Error::invalid_request(),
            )),
        }
    }
    peer.fail_all();
    for (_, task) in std::mem::take(&mut *in_flight.lock().expect("in-flight map poisoned")) {
        task.abort();
    }
}

fn cancel(in_flight: &InFlight, params: &serde_json::Value) {
    let Ok(cydonia_acp_proto::CancelRequestNotification { request_id, .. }) =
        serde_json::from_value(params.clone())
    else {
        return;
    };
    let task = in_flight
        .lock()
        .expect("in-flight map poisoned")
        .remove(&request_id);
    if let Some(task) = task {
        task.abort();
    }
}

enum Frame {
    Request {
        id: cydonia_acp_proto::RequestId,
        method: String,
        params: serde_json::Value,
    },
    Notification {
        method: String,
        params: serde_json::Value,
    },
    Response {
        id: cydonia_acp_proto::RequestId,
        outcome: Result<serde_json::Value, cydonia_acp_proto::Error>,
    },
    Ignore,
    Invalid,
}

fn classify(message: Message) -> Frame {
    let Message {
        id,
        method,
        params,
        result,
        error,
        ..
    } = message;
    let params = params.unwrap_or(serde_json::Value::Null);
    match (id, method) {
        (Some(id), Some(method)) => Frame::Request { id, method, params },
        (None, Some(method)) => Frame::Notification { method, params },
        // A reply we cannot correlate — the peer's own error about a frame it
        // could not parse. Answering it would start a volley of errors.
        (None, None) if result.is_some() || error.is_some() => Frame::Ignore,
        (Some(id), None) => {
            let outcome = match error {
                Some(e) => Err(e),
                // An omitted `result` on a successful response means "no
                // payload"; every empty ACP response deserializes from `{}`.
                None => Ok(result.unwrap_or_else(|| serde_json::json!({}))),
            };
            Frame::Response { id, outcome }
        }
        (None, None) => Frame::Invalid,
    }
}
