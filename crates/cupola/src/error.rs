/// What a Cupola call fails with.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("i/o: {0}")]
    Io(#[from] std::io::Error),
    #[error("encode: {0}")]
    Encode(postcard::Error),
    #[error("decode: {0}")]
    Decode(postcard::Error),
    #[error("frame of {0} bytes is over the limit")]
    Oversized(usize),
    /// The other side has no handler registered under this name.
    #[error("no service named {0}")]
    NoService(String),
    /// The other side's handler, or its decoding of the request, failed.
    #[error("{service}: {message}")]
    Remote { service: String, message: String },
    /// The client or server this call went through was dropped, or the
    /// client was closed.
    #[error("closed")]
    Closed,
    /// The server was started afresh before it answered; the request may or
    /// may not have run on the one before.
    #[error("the server was restarted")]
    Restarted,
}
