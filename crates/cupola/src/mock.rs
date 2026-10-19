//! An in-process transport: two [`Connection`]s joined by memory, with no
//! process or network between them.

use std::{
    io,
    pin::Pin,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    task::{Context, Poll},
};

use futures::{
    AsyncRead, AsyncWrite, StreamExt,
    channel::mpsc::{self, UnboundedReceiver, UnboundedSender},
};

use crate::Connection;

/// The memory between one pair of mock connections.
pub struct Wire {
    lanes: [UnboundedSender<Vec<u8>>; 2],
    severed: Arc<AtomicBool>,
}

struct Writer {
    lane: UnboundedSender<Vec<u8>>,
    severed: Arc<AtomicBool>,
}

struct Reader {
    lane: UnboundedReceiver<Vec<u8>>,
    chunk: Vec<u8>,
    read: usize,
    severed: Arc<AtomicBool>,
}

impl Wire {
    /// Two connected ends: what one writes, the other reads. Dropping either
    /// end's connection ends the other's reader.
    pub fn pair() -> (Self, Connection, Connection) {
        let severed = Arc::new(AtomicBool::new(false));
        let (left_out, right_in) = mpsc::unbounded();
        let (right_out, left_in) = mpsc::unbounded();
        let end = |lane_in, lane_out: &UnboundedSender<Vec<u8>>| {
            Connection::new(
                Reader {
                    lane: lane_in,
                    chunk: Vec::new(),
                    read: 0,
                    severed: severed.clone(),
                },
                Writer {
                    lane: lane_out.clone(),
                    severed: severed.clone(),
                },
            )
        };
        let left = end(left_in, &left_out);
        let right = end(right_in, &right_out);
        let wire = Self {
            lanes: [left_out, right_out],
            severed,
        };
        (wire, left, right)
    }

    /// Cuts the wire: both readers reach end of stream, bytes not yet read
    /// are lost, and later writes succeed and go nowhere.
    pub fn sever(&self) {
        self.severed.store(true, Ordering::SeqCst);
        for lane in &self.lanes {
            lane.close_channel();
        }
    }
}

impl AsyncWrite for Writer {
    fn poll_write(
        self: Pin<&mut Self>,
        _: &mut Context<'_>,
        bytes: &[u8],
    ) -> Poll<io::Result<usize>> {
        if !self.severed.load(Ordering::SeqCst) {
            let _ = self.lane.unbounded_send(bytes.to_vec());
        }
        Poll::Ready(Ok(bytes.len()))
    }

    fn poll_flush(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(Ok(()))
    }

    fn poll_close(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<io::Result<()>> {
        self.lane.close_channel();
        Poll::Ready(Ok(()))
    }
}

impl Drop for Writer {
    fn drop(&mut self) {
        self.lane.close_channel();
    }
}

impl AsyncRead for Reader {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buffer: &mut [u8],
    ) -> Poll<io::Result<usize>> {
        loop {
            if self.severed.load(Ordering::SeqCst) {
                return Poll::Ready(Ok(0));
            }
            if self.read < self.chunk.len() {
                let n = buffer.len().min(self.chunk.len() - self.read);
                buffer[..n].copy_from_slice(&self.chunk[self.read..self.read + n]);
                self.read += n;
                return Poll::Ready(Ok(n));
            }
            match self.lane.poll_next_unpin(cx) {
                Poll::Ready(Some(chunk)) => {
                    self.chunk = chunk;
                    self.read = 0;
                }
                Poll::Ready(None) => return Poll::Ready(Ok(0)),
                Poll::Pending => return Poll::Pending,
            }
        }
    }
}
