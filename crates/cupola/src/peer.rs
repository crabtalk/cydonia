//! One side's envelope bookkeeping, shared by the client and the server:
//! ids, acknowledgement, the resend buffer and the attached connection.

use std::{
    collections::VecDeque,
    pin::pin,
    sync::{Mutex, MutexGuard},
};

use futures::{
    StreamExt,
    channel::mpsc::{self, UnboundedSender},
    future::{Either, select},
};

use crate::{
    Connection, Error,
    wire::{Envelope, Kind},
};

pub(crate) struct Peer {
    state: Mutex<State>,
}

#[derive(Default)]
struct State {
    /// The id of the last envelope sent.
    sent: u64,
    /// The highest id received from the peer.
    received: u64,
    /// Sent and not acknowledged, in id order.
    unacked: VecDeque<Envelope>,
    generation: u64,
    live: Option<Live>,
}

struct Live {
    generation: u64,
    outgoing: UnboundedSender<Outgoing>,
}

enum Outgoing {
    Envelope(Envelope),
    /// Acknowledge what has been received, unless a later write already has.
    Ack,
}

impl Peer {
    pub(crate) fn new() -> Self {
        Self {
            state: Mutex::new(State::default()),
        }
    }

    fn state(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap()
    }

    /// Queues an envelope and returns its id. It is written now if a
    /// connection is attached, and again on every attach until acknowledged.
    /// One that could encode over [`MAX_FRAME`](crate::wire::MAX_FRAME) is
    /// refused and never queued.
    pub(crate) fn send(&self, service: &str, kind: Kind, payload: Vec<u8>) -> Result<u64, Error> {
        Envelope::fits(service, &payload)?;
        let mut state = self.state();
        state.sent += 1;
        let envelope = Envelope {
            id: state.sent,
            ack_id: 0,
            service: service.to_owned(),
            kind,
            payload,
        };
        if let Some(live) = &state.live {
            let _ = live
                .outgoing
                .unbounded_send(Outgoing::Envelope(envelope.clone()));
        }
        state.unacked.push_back(envelope);
        Ok(state.sent)
    }

    /// Forgets what was received and drops what was not acknowledged, for a
    /// peer that starts afresh. Ids sent go on increasing. Called while no
    /// connection is attached.
    pub(crate) fn reset(&self) {
        let mut state = self.state();
        state.received = 0;
        state.unacked.clear();
    }

    /// Runs `connection` until either stream ends or fails, handing every
    /// envelope not seen before to `receive`. Attaching while another
    /// connection is attached stops writing to the earlier one.
    pub(crate) async fn attach(
        &self,
        connection: Connection,
        receive: impl Fn(Envelope),
    ) -> Result<(), Error> {
        let (outgoing, mut queue) = mpsc::unbounded();
        let generation = {
            let mut state = self.state();
            state.generation += 1;
            let _ = outgoing.unbounded_send(Outgoing::Ack);
            for envelope in &state.unacked {
                let _ = outgoing.unbounded_send(Outgoing::Envelope(envelope.clone()));
            }
            state.live = Some(Live {
                generation: state.generation,
                outgoing,
            });
            state.generation
        };
        let Connection {
            mut reader,
            mut writer,
        } = connection;

        let write = async {
            let mut acked = 0;
            while let Some(outgoing) = queue.next().await {
                let received = self.state().received;
                let mut envelope = match outgoing {
                    Outgoing::Envelope(envelope) => envelope,
                    Outgoing::Ack if received > acked => Envelope::ack(),
                    Outgoing::Ack => continue,
                };
                envelope.ack_id = received;
                acked = received;
                envelope.write(&mut writer).await?;
            }
            Ok(())
        };
        let read = async {
            while let Some(envelope) = Envelope::read(&mut reader).await? {
                if self.accept(&envelope) {
                    receive(envelope);
                }
            }
            Ok(())
        };
        let result = match select(pin!(read), pin!(write)).await {
            Either::Left((result, _)) | Either::Right((result, _)) => result,
        };

        let mut state = self.state();
        if state
            .live
            .as_ref()
            .is_some_and(|live| live.generation == generation)
        {
            state.live = None;
        }
        result
    }

    /// Drops what `envelope` acknowledges from the resend buffer, and answers
    /// whether it is new.
    fn accept(&self, envelope: &Envelope) -> bool {
        let mut state = self.state();
        while state
            .unacked
            .front()
            .is_some_and(|sent| sent.id <= envelope.ack_id)
        {
            state.unacked.pop_front();
        }
        if envelope.kind == Kind::Ack || envelope.id <= state.received {
            return false;
        }
        state.received = envelope.id;
        if let Some(live) = &state.live {
            let _ = live.outgoing.unbounded_send(Outgoing::Ack);
        }
        true
    }
}
