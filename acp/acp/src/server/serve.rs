//! Adapts an [`Agent`] to the connection's inbound half.

use crate::{Handler, server::Agent};
use std::{future::Future, sync::Arc};

pub(crate) struct Serve<A>(pub Arc<A>);

impl<A: Agent> Handler for Serve<A> {
    fn request(
        &self,
        method: String,
        params: serde_json::Value,
    ) -> impl Future<Output = cydonia_acp_proto::Result<serde_json::Value>> + Send {
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
