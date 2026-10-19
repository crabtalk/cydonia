use std::pin::Pin;

use futures::{AsyncRead, AsyncWrite};

/// One transport's pair of byte streams between a client and a server.
///
/// A reader at end of stream ends the attachment it was handed to.
pub struct Connection {
    pub(crate) reader: Pin<Box<dyn AsyncRead + Send>>,
    pub(crate) writer: Pin<Box<dyn AsyncWrite + Send>>,
}

impl Connection {
    pub fn new(
        reader: impl AsyncRead + Send + 'static,
        writer: impl AsyncWrite + Send + 'static,
    ) -> Self {
        Self {
            reader: Box::pin(reader),
            writer: Box::pin(writer),
        }
    }
}
