use serde::{Serialize, de::DeserializeOwned};

/// A protocol extension: the name envelopes are routed by, and the types its
/// payloads decode to.
///
/// The side that answers a service and the side that calls it agree on it
/// only by `NAME` and by the encoding of these types; both sides must build
/// from one definition.
pub trait Service: 'static {
    /// Unique among the services one side registers.
    const NAME: &'static str;
    /// What the calling side asks.
    type Request: Serialize + DeserializeOwned + Send + 'static;
    /// What the answering side answers one request with.
    type Response: Serialize + DeserializeOwned + Send + 'static;
    /// What is pushed unasked.
    type Event: Serialize + DeserializeOwned + Send + 'static;
}
