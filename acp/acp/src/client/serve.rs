//! Adapts a [`Client`] to the connection's inbound half.

use crate::{Handler, client::Client};
use std::{future::Future, sync::Arc};

pub(crate) struct Serve<C>(pub Arc<C>);

impl<C: Client> Handler for Serve<C> {
    fn request(
        &self,
        method: String,
        params: serde_json::Value,
    ) -> impl Future<Output = acp_proto::Result<serde_json::Value>> + Send {
        self.0.dispatch(method, params)
    }

    fn notification(
        &self,
        method: String,
        params: serde_json::Value,
    ) -> impl Future<Output = ()> + Send {
        self.0.dispatch_notification(method, params)
    }
}
