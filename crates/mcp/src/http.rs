//! The door: Streamable HTTP on loopback, one endpoint for the whole app.
//!
//! One address and no configuration. The port is whatever the system hands
//! out, because nobody types it — the app hands the URL to the agents it
//! launches and there is nothing else to give it to.
//!
//! There is no token. A process running as this user can already write every
//! file these tools write, so a gate between them buys nothing. A *page* in a
//! browser cannot, which is why the one check here is on `Origin`: a request
//! carrying one came from somewhere with a name, and this server has no
//! business answering it.

use crate::{Server, proto::Request};
use axum::{
    Router,
    extract::State,
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
    routing::post,
};
use std::{
    net::{Ipv4Addr, SocketAddr},
    sync::Arc,
};
use tokio::{net::TcpListener, sync::oneshot};

/// Where the tools answer.
pub const PATH: &str = "/mcp";

/// The only address this ever binds. Named here rather than passed in: a
/// server whose tools edit the user's files has no business being reachable
/// from the network, and a caller that could choose would eventually choose
/// wrong.
const HOST: Ipv4Addr = Ipv4Addr::LOCALHOST;

/// A door that is open. Dropping it closes the listener, so whatever holds it
/// decides how long it lives and nothing else has to remember.
pub struct Door {
    url: String,
    close: Option<oneshot::Sender<()>>,
}

impl Door {
    /// Where the tools are, as it goes to an agent.
    pub fn url(&self) -> &str {
        &self.url
    }
}

impl Drop for Door {
    fn drop(&mut self) {
        if let Some(close) = self.close.take() {
            let _ = close.send(());
        }
    }
}

/// Bind a loopback port and start answering.
///
/// The port is the system's to choose. Awaited rather than spawned so a bind
/// that fails is an error where it can be reported, though there is little to
/// fail on a port nobody asked for.
pub async fn open(server: Arc<Server>) -> std::io::Result<Door> {
    let listener = TcpListener::bind(SocketAddr::from((HOST, 0))).await?;
    let url = format!("http://{}{PATH}", listener.local_addr()?);
    let router = Router::new()
        .route(PATH, post(call).get(no_stream))
        .with_state(server);
    let (close, closed) = oneshot::channel();
    tokio::spawn(async move {
        let _ = axum::serve(listener, router)
            .with_graceful_shutdown(async {
                let _ = closed.await;
            })
            .await;
    });
    Ok(Door {
        url,
        close: Some(close),
    })
}

/// One MCP call.
async fn call(State(server): State<Arc<Server>>, headers: HeaderMap, body: String) -> Response {
    // A client that is not a browser sends no `Origin` at all. One that does is
    // a page, and a page reaching a loopback port is the rebinding attack this
    // check exists for — the name it resolved is not one we can vouch for, so
    // the header is refused rather than matched.
    if headers.contains_key(header::ORIGIN) {
        return (StatusCode::FORBIDDEN, "this server is not for a browser").into_response();
    }
    let Ok(request) = serde_json::from_str::<Request>(&body) else {
        return (StatusCode::BAD_REQUEST, "not a JSON-RPC request").into_response();
    };
    match server.handle(&request) {
        Some(response) => axum::Json(response).into_response(),
        // A notification is answered by not answering, which over HTTP is the
        // status that says so.
        None => StatusCode::ACCEPTED.into_response(),
    }
}

/// The GET half of Streamable HTTP is the server's own stream, and this server
/// has nothing to push — `listChanged` is false for the same reason.
async fn no_stream() -> Response {
    (StatusCode::METHOD_NOT_ALLOWED, "this server pushes nothing").into_response()
}
