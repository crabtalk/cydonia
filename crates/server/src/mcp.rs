//! The `mcp` Cupola service: one MCP message, answered by the client with
//! cydonia's MCP server. An agent on the server's machine reaches it through
//! [`Command::Mcp`](crate::Command::Mcp), which joins the running daemon and
//! relays its stdio over [`stdio`]; the daemon passes each request on to its
//! client with [`Mcp::relay`].

use std::{
    ffi::OsString,
    io::{self, BufRead as _, BufReader, Read, Write},
    path::PathBuf,
    pin::pin,
    sync::Arc,
};

use cupola::{Client, Connection, Error, Server, Service};
use futures::{
    StreamExt as _,
    channel::mpsc,
    future::{self, Either},
    stream::FuturesUnordered,
};
use mcp::proto::{self, Response};
use serde::{Deserialize, Serialize};

const PROJECT: &str = "--project";
const SESSION: &str = "--session";

/// cydonia's MCP server, answered by the client.
pub struct Mcp;

/// What every message relayed for one agent is about.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Binding {
    /// The project the agent runs in, a directory on the server's machine.
    pub project: Option<PathBuf>,
    /// The record id of the session the agent is.
    pub session: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Request {
    pub binding: Binding,
    /// One JSON-RPC message, as MCP's stdio transport frames it, without its
    /// newline.
    pub message: String,
}

impl Binding {
    /// The binding `args` name: `--project <dir>` and `--session <id>`,
    /// each at most once, in either order.
    pub fn parse(args: &[&str]) -> Option<Self> {
        let mut binding = Self::default();
        let mut args = args.iter();
        while let Some(flag) = args.next() {
            let value = *args.next()?;
            match *flag {
                PROJECT if binding.project.is_none() => binding.project = Some(value.into()),
                SESSION if binding.session.is_none() => binding.session = Some(value.to_owned()),
                _ => return None,
            }
        }
        Some(binding)
    }

    /// The arguments [`Binding::parse`] reads as this binding.
    pub fn args(&self) -> Vec<OsString> {
        let mut args = Vec::new();
        if let Some(project) = &self.project {
            args.extend([OsString::from(PROJECT), project.into()]);
        }
        if let Some(session) = &self.session {
            args.extend([OsString::from(SESSION), session.into()]);
        }
        args
    }
}

impl Service for Mcp {
    const NAME: &'static str = "mcp";
    type Request = Request;
    /// The message that answers the request's, none for a notification.
    type Response = Option<String>;
    type Event = ();
}

impl Mcp {
    /// Answers [`Mcp`] requests on `client` with `answer`, each on a blocking
    /// thread of its own.
    pub fn serve(
        client: &Client,
        answer: impl Fn(Request) -> Option<String> + Send + Sync + 'static,
    ) {
        let answer = Arc::new(answer);
        client.register::<Self, _, _>(move |request: Request| {
            let answer = answer.clone();
            async move { Ok(blocking::unblock(move || answer(request)).await) }
        });
    }

    /// Answers [`Mcp`] requests on `local`, a server the daemon made for a
    /// process on its machine, by sending each to the client of `server`.
    pub fn relay(local: &Server, server: &Server) {
        let client = server.service::<Self>();
        local.register::<Self, _, _>(move |request: Request| {
            let answer = client.send(&request);
            async move { Ok(answer.await?) }
        });
    }
}

/// Relays the messages read from `input` through `connection`, a joined
/// daemon's, each carrying `binding`, and writes their answers to `output`.
/// Messages are newline-delimited, as MCP's stdio transport frames them. A
/// request that gets no answer through the daemon is answered with an
/// internal error.
///
/// `input` is read on a thread of its own. Ends once `input` has ended and
/// every message read from it is answered, or when the connection ends
/// first.
pub async fn stdio(
    connection: Connection,
    binding: Binding,
    input: impl Read + Send + 'static,
    mut output: impl Write,
) -> anyhow::Result<()> {
    let (client, runner) = Client::new();
    let handle = client.service::<Mcp>();
    let (lines, mut read) = mpsc::unbounded();
    std::thread::spawn(move || {
        for line in BufReader::new(input).lines() {
            let Ok(line) = line else { return };
            if lines.unbounded_send(line).is_err() {
                return;
            }
        }
    });
    let relayed = async {
        let mut waiting = FuturesUnordered::new();
        let mut open = true;
        while open || !waiting.is_empty() {
            let next = match (open, waiting.is_empty()) {
                (true, true) => Either::Left(read.next().await),
                (false, _) => Either::Right(waiting.next().await),
                (true, false) => match future::select(read.next(), waiting.next()).await {
                    Either::Left((line, _)) => Either::Left(line),
                    Either::Right((answered, _)) => Either::Right(answered),
                },
            };
            match next {
                Either::Left(Some(line)) if line.trim().is_empty() => {}
                Either::Left(Some(line)) => {
                    let request = Request {
                        binding: binding.clone(),
                        message: line,
                    };
                    let answer = handle.send(&request);
                    waiting.push(async move { (request.message, answer.await) });
                }
                Either::Left(None) => open = false,
                Either::Right(Some((message, answer))) => {
                    if let Some(reply) = reply(&message, answer) {
                        writeln!(output, "{reply}")?;
                        output.flush()?;
                    }
                }
                Either::Right(None) => {}
            }
        }
        io::Result::Ok(())
    };
    let attached = future::join(runner, client.attach(connection));
    match future::select(pin!(relayed), pin!(attached)).await {
        Either::Left((relayed, _)) => Ok(relayed?),
        Either::Right(_) => anyhow::bail!("the daemon closed the connection"),
    }
}

/// What answers `message`, given what came back through the daemon.
fn reply(message: &str, answer: Result<Option<String>, Error>) -> Option<String> {
    match answer {
        Ok(reply) => reply,
        Err(error) => {
            let id = serde_json::from_str::<proto::Request>(message).ok()?.id?;
            let error = proto::Error::new(
                proto::INTERNAL_ERROR,
                format!("cydonia did not answer: {error}"),
            );
            serde_json::to_string(&Response::fail(id, error)).ok()
        }
    }
}
