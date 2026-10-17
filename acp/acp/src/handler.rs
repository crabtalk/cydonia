//! What a side of the connection answers when the other side calls.

use std::future::Future;

/// The inbound half of a connection.
///
/// Implemented for you by the `server` and `client` modules; you should not need
/// to write it by hand.
pub trait Handler: Send + Sync + 'static {
    /// Answer a request. Returning `Err` sends that error back — it never
    /// tears down the connection.
    fn request(
        &self,
        method: String,
        params: serde_json::Value,
    ) -> impl Future<Output = acp_proto::Result<serde_json::Value>> + Send;

    /// Handle a notification. Awaited inline, so notifications stay in wire
    /// order; keep it quick.
    fn notification(
        &self,
        method: String,
        params: serde_json::Value,
    ) -> impl Future<Output = ()> + Send;
}

/// Params that don't match the method's shape are the caller's error.
pub(crate) fn decode<T: serde::de::DeserializeOwned>(
    params: serde_json::Value,
) -> acp_proto::Result<T> {
    serde_json::from_value(params)
        .map_err(|e| acp_proto::Error::invalid_params().data(e.to_string()))
}

/// A handler returning something unserializable is our bug.
pub(crate) fn encode<T: serde::Serialize>(value: T) -> acp_proto::Result<serde_json::Value> {
    serde_json::to_value(value).map_err(|e| acp_proto::Error::internal_error().data(e.to_string()))
}
