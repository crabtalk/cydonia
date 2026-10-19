//! The frames on the byte stream: a little-endian `u32` length, then a
//! postcard-encoded [`Envelope`] of that many bytes.

use std::io;

use futures::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use serde::{Deserialize, Serialize};

use crate::Error;

/// The largest encoded envelope [`Envelope::read`] accepts.
pub const MAX_FRAME: u32 = 64 << 20;

/// The most an envelope's encoding adds to its service name and payload:
/// postcard varints of at most 10 bytes for the id, `ack_id`, a response's
/// `to` and the two lengths, and 5 for the kind's tag.
const OVERHEAD: usize = 5 * 10 + 5;

/// One message between a client and a server, either way.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Envelope {
    /// 0 on an [`Kind::Ack`]. Otherwise unique per sender, increasing from 1
    /// in the order envelopes are written.
    pub id: u64,
    /// The highest id the sender has received from the peer, 0 for none.
    pub ack_id: u64,
    /// The [`Service::NAME`](crate::Service::NAME) the payload belongs to,
    /// empty on an [`Kind::Ack`].
    pub service: String,
    pub kind: Kind,
    pub payload: Vec<u8>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Kind {
    /// Carries only `ack_id`; never kept for resending.
    Ack,
    Request,
    /// The answer to the request whose id is `to`.
    Response {
        to: u64,
    },
    /// The request whose id is `to` got no response; the payload is a
    /// [`Fault`].
    Failure {
        to: u64,
    },
    /// Pushed unasked.
    Event,
}

/// The payload of a [`Kind::Failure`].
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Fault {
    NoService,
    Failed(String),
}

impl Envelope {
    /// Refuses a service name and payload whose envelope could encode over
    /// [`MAX_FRAME`].
    pub fn fits(service: &str, payload: &[u8]) -> Result<(), Error> {
        let bound = service.len() + payload.len() + OVERHEAD;
        if bound > MAX_FRAME as usize {
            return Err(Error::Oversized(bound));
        }
        Ok(())
    }

    pub fn ack() -> Self {
        Self {
            id: 0,
            ack_id: 0,
            service: String::new(),
            kind: Kind::Ack,
            payload: Vec::new(),
        }
    }

    /// The next envelope on the stream, or `None` when the stream ends
    /// between frames.
    pub async fn read(reader: &mut (impl AsyncRead + Unpin)) -> Result<Option<Self>, Error> {
        let mut prefix = [0; 4];
        let mut filled = 0;
        while filled < prefix.len() {
            match reader.read(&mut prefix[filled..]).await? {
                0 if filled == 0 => return Ok(None),
                0 => return Err(io::Error::from(io::ErrorKind::UnexpectedEof).into()),
                n => filled += n,
            }
        }
        let len = u32::from_le_bytes(prefix);
        if len > MAX_FRAME {
            return Err(Error::Oversized(len as usize));
        }
        let mut body = vec![0; len as usize];
        reader.read_exact(&mut body).await?;
        postcard::from_bytes(&body).map(Some).map_err(Error::Decode)
    }

    /// Writes one frame and flushes.
    pub async fn write(&self, writer: &mut (impl AsyncWrite + Unpin)) -> Result<(), Error> {
        let body = postcard::to_allocvec(self).map_err(Error::Encode)?;
        let len = match u32::try_from(body.len()) {
            Ok(len) if len <= MAX_FRAME => len,
            _ => return Err(Error::Oversized(body.len())),
        };
        writer.write_all(&len.to_le_bytes()).await?;
        writer.write_all(&body).await?;
        writer.flush().await?;
        Ok(())
    }
}
