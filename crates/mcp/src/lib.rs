//! The MCP server cydonia answers as: the tools an agent works a project
//! through.
//!
//! What carried a call is not a tool's business. [`Server::handle`] takes a
//! request that is already decoded and answers one, so the HTTP door hands it
//! a parsed body and the ACP tunnel hands it params that were never framed at
//! all — and neither of them is named in here.
//!
//! Nothing here holds a project. One server answers for the whole app, and
//! which directory a call is about is an argument on the call — so there is no
//! registry of what is open, and a tool reaches a project the app never opened
//! the same way it reaches one it did.

pub mod http;
pub mod proto;
pub mod rail;
mod resources;
pub mod tool;
pub mod tools;

use proto::{Error, Request, Response};
use serde_json::{Value, json};
use std::path::Path;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use tool::{Answer, Args, Tool, Trouble};

/// What the server calls itself in `initialize`.
const NAME: &str = "cydonia";

pub struct Server {
    /// Mounted rather than compiled in. A surface the user has switched off is
    /// one the agent must not be told about at all — a tool that is listed and
    /// refuses has already cost the turn its tokens.
    tools: Vec<&'static Tool>,
    /// Whether the tools that take an entry off the disk are offered.
    deletes: Arc<AtomicBool>,
    /// Whether the tools that change a project are offered.
    ///
    /// Shared and read per call rather than settled when the server is built:
    /// an agent is holding the URL, so a switch that rebound the port to take
    /// effect would be a switch that broke every live session.
    writable: Arc<AtomicBool>,
}

impl Default for Server {
    fn default() -> Self {
        Self {
            tools: Vec::new(),
            writable: Arc::new(AtomicBool::new(true)),
            // Off, unlike writing: a caller with no switch of its own gets a
            // server that cannot empty a project.
            deletes: Arc::new(AtomicBool::new(false)),
        }
    }
}

impl Server {
    pub fn new() -> Self {
        Self::default()
    }

    /// Take the write switch from whoever owns it. Without this a server is
    /// writable, which is what a caller that has no switch means.
    pub fn writable(mut self, switch: Arc<AtomicBool>) -> Self {
        self.writable = switch;
        self
    }

    /// Take the delete switch from whoever owns it. Without this a server
    /// deletes nothing, which is what a caller that has no switch means: a
    /// tool that cannot be undone is not one to be given by default.
    pub fn deletes(mut self, switch: Arc<AtomicBool>) -> Self {
        self.deletes = switch;
        self
    }

    /// The tools on offer this call.
    ///
    /// Left out of the listing rather than refused on the call — see
    /// [`Server::tools`]. A deletion is gated twice over: writing off takes
    /// the delete tools with it, because a server that may not change a
    /// project certainly may not empty one.
    fn offered(&self) -> impl Iterator<Item = &&'static Tool> {
        let writable = self.writable.load(Ordering::Relaxed);
        let deletes = self.deletes.load(Ordering::Relaxed);
        self.tools
            .iter()
            .filter(move |tool| writable || !tool.writes)
            .filter(move |tool| (writable && deletes) || !tool.deletes)
    }

    /// Offer a set of tools. Called once per surface the features leave on.
    pub fn mount(mut self, tools: &'static [Tool]) -> Self {
        self.tools.extend(tools);
        self
    }

    /// Answer one request, or nothing where the wire expects nothing.
    ///
    /// `at` is the project the caller was opened in, when it was opened in one
    /// — a session belongs to a project and the app knows which, so the client
    /// is told rather than the model asked. `None` is a caller that reached
    /// the port on its own and has to name a directory per call.
    ///
    /// A frame with no id is a notification whatever its method is —
    /// `notifications/initialized` is the one that arrives — and answering one
    /// is a protocol error rather than a courtesy.
    pub fn handle(&self, request: &Request, at: Option<&Path>) -> Option<Response> {
        self.handle_from(request, at, None)
    }

    /// [`Self::handle`] for a call made by the session filed under `session`.
    pub fn handle_from(
        &self,
        request: &Request,
        at: Option<&Path>,
        session: Option<&str>,
    ) -> Option<Response> {
        let id = request.id.clone()?;
        Some(match request.method.as_str() {
            "initialize" => Response::ok(id, self.initialize(at)),
            "ping" => Response::ok(id, json!({})),
            "tools/list" => Response::ok(id, self.list(at.is_some())),
            "resources/list" => match resources::list(request.params.as_ref()) {
                Ok(result) => Response::ok(id, result),
                Err(error) => Response::fail(id, error),
            },
            "resources/read" => match resources::read(request.params.as_ref()) {
                Ok(result) => Response::ok(id, result),
                Err(error) => Response::fail(id, error),
            },
            "resources/templates/list" => Response::ok(id, json!({ "resourceTemplates": [] })),
            "tools/call" => match self.invoke(request.params.as_ref(), at, session) {
                Ok(result) => Response::ok(id, result),
                Err(error) => Response::fail(id, error),
            },
            method => Response::fail(id, Error::method_not_found(method)),
        })
    }

    /// Run one tool by name. The surface the tests drive, and what
    /// `tools/call` is a wire around.
    pub fn call(&self, name: &str, arguments: Value, at: Option<&Path>) -> Result<Answer, Trouble> {
        self.call_from(name, arguments, at, None)
    }

    /// [`Self::call`] for a call made by the session filed under `session`.
    pub fn call_from(
        &self,
        name: &str,
        arguments: Value,
        at: Option<&Path>,
        session: Option<&str>,
    ) -> Result<Answer, Trouble> {
        if let Some(tool) = self.offered().find(|tool| tool.name == name) {
            return (tool.call)(Args::new(&arguments, at).from_session(session));
        }
        // Withheld rather than absent, which is worth saying: the model asked
        // for something that exists and is switched off, and a flat "no such
        // tool" would have it hunting for the right name.
        if let Some(tool) = self.tools.iter().find(|tool| tool.name == name) {
            // Which switch is holding it: a delete tool refused for the wrong
            // reason would have somebody turning on the wrong one.
            let why = match tool.deletes && self.writable.load(Ordering::Relaxed) {
                true => "deletes, and cydonia is set not to let agents delete",
                false => "changes the project, and cydonia is set to let agents read only",
            };
            return Err(Trouble::Refused(format!("{name} {why}")));
        }
        Err(Trouble::Invalid(format!("no tool {name}")))
    }

    fn initialize(&self, at: Option<&Path>) -> Value {
        let instructions = prompts::tool_context(at.is_some());
        let capabilities = json!({ "tools": { "listChanged": false }, "resources": {} });
        json!({
            "protocolVersion": proto::VERSION,
            // `listChanged` is a promise to send a notification, and this
            // server has nowhere to send one from until the door grows a
            // stream. Saying false is what keeps a client from waiting for it.
            "capabilities": capabilities,
            // The app's own version. `protocolVersion` above is the spec
            // revision the client matches against, and is not ours to name.
            "serverInfo": { "name": NAME, "version": env!("CARGO_PKG_VERSION") },
            "instructions": instructions,
        })
    }

    fn list(&self, bound: bool) -> Value {
        json!({
            "tools": self
                .offered()
                .map(|tool| json!({
                    "name": tool.name,
                    "description": tool.description,
                    "inputSchema": (tool.schema)(bound),
                }))
                .collect::<Vec<_>>(),
        })
    }

    /// `tools/call`, and the split that makes the surface usable: a malformed
    /// call is a JSON-RPC error, and a call that was fine but got no for an
    /// answer is a result carrying `isError` — which the model sees and can do
    /// something about.
    fn invoke(
        &self,
        params: Option<&Value>,
        at: Option<&Path>,
        session: Option<&str>,
    ) -> Result<Value, Error> {
        let params = params.ok_or_else(|| Error::invalid_params("no params"))?;
        let name = params
            .get("name")
            .and_then(Value::as_str)
            .ok_or_else(|| Error::invalid_params("name is required, as a string"))?;
        let arguments = params
            .get("arguments")
            .cloned()
            .unwrap_or_else(|| json!({}));
        match self.call_from(name, arguments, at, session) {
            Ok(answer) => Ok(result(&answer.text, answer.data, false)),
            Err(Trouble::Refused(why)) => Ok(result(&why, None, true)),
            Err(Trouble::Invalid(why)) => Err(Error::invalid_params(why)),
        }
    }
}

/// A `tools/call` result: the text a model reads, and the shape beside it.
fn result(text: &str, data: Option<Value>, failed: bool) -> Value {
    let mut result = json!({ "content": [{ "type": "text", "text": text }] });
    if let Some(data) = data {
        result["structuredContent"] = data;
    }
    // Left out when it is false, which is what it defaults to.
    if failed {
        result["isError"] = Value::Bool(true);
    }
    result
}
