//! The door: Streamable HTTP on loopback, one endpoint for the whole app.
//!
//! One address, and the same one next launch. The port is pinned rather than
//! asked of the system: a client keeps the URL in its own config file, and a
//! number that moved every restart would be a client that had to be told again
//! every restart. Something else holding it is answered by stepping past it,
//! not by giving up and not by drifting somewhere nobody will look.
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

/// The port the door prefers. Pinned so the address survives a restart, which
/// is what lets a client be configured once.
pub const PORT: u16 = 7457;

/// How far past it the door will walk when something already holds one. A
/// second cydonia is the usual reason, and it takes the next number rather
/// than failing — but a walk that went on forever would land somewhere nobody
/// was told about, so it stops.
const SPAN: u16 = 16;

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

/// Bind the pinned port and start answering, stepping past whatever is already
/// holding it — see [`PORT`] and [`SPAN`].
///
/// The error handed back is the last one, which is the honest thing to report:
/// every port in the range was refused, and the reason the last was refused is
/// as good an account as any.
pub async fn open(server: Arc<Server>) -> std::io::Result<Door> {
    let mut refused = None;
    for port in PORT..PORT.saturating_add(SPAN) {
        match open_at(port, server.clone()).await {
            Ok(door) => return Ok(door),
            Err(why) => refused = Some(why),
        }
    }
    Err(refused.unwrap_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::AddrInUse,
            "no port in the range is free",
        )
    }))
}

/// The same, on exactly the port asked for. Zero is the system's to choose,
/// which is what a test wants and what the app never does.
pub async fn open_at(port: u16, server: Arc<Server>) -> std::io::Result<Door> {
    let listener = TcpListener::bind(SocketAddr::from((HOST, port))).await?;
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
