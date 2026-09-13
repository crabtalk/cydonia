//! The MCP server cydonia answers as: the tools an agent works a project
//! through.
//!
//! What carried a call is not a tool's business. [`Server::handle`] takes a
//! request that is already decoded and answers one, so the HTTP door hands it
//! a parsed body and the ACP tunnel hands it params that were never framed at
//! all — and neither of them is named in here.
//!
//! Nothing here knows what a window is. The server answers an
//! [`artifact::project::Project`], which is the same trait the app's own store
//! implements and the same one a directory in `/tmp` does — which is what lets
//! the whole surface be tested without one.

pub mod proto;
pub mod tool;
pub mod tools;

use artifact::project::Project;
use proto::{Error, Request, Response};
use serde_json::{Value, json};
use std::sync::Arc;
use tool::{Answer, Args, Tool, Trouble};

/// What the server calls itself in `initialize`.
const NAME: &str = "cydonia";

/// What it tells a model it is holding, once, instead of in every description.
const INSTRUCTIONS: &str = "\
The boards of one cydonia project, as the app holds them. A board is named by \
its key (ROAD), its name or its id; a card by its handle (ROAD-12) or its id; \
a column by its name or its id. A handle names one card across the whole \
project, so no tool that takes a card also takes a board.";

pub struct Server {
    project: Arc<dyn Project + Send + Sync>,
    /// Mounted rather than compiled in. A surface the user has switched off is
    /// one the agent must not be told about at all — a tool that is listed and
    /// refuses has already cost the turn its tokens.
    tools: Vec<&'static Tool>,
}

impl Server {
    pub fn new(project: Arc<dyn Project + Send + Sync>) -> Self {
        Self {
            project,
            tools: Vec::new(),
        }
    }

    /// Offer a set of tools. Called once per surface the features leave on.
    pub fn mount(mut self, tools: &'static [Tool]) -> Self {
        self.tools.extend(tools);
        self
    }

    /// Answer one request, or nothing where the wire expects nothing.
    ///
    /// A frame with no id is a notification whatever its method is —
    /// `notifications/initialized` is the one that arrives — and answering one
    /// is a protocol error rather than a courtesy.
    pub fn handle(&self, request: &Request) -> Option<Response> {
        let id = request.id.clone()?;
        Some(match request.method.as_str() {
            "initialize" => Response::ok(id, self.initialize()),
            "ping" => Response::ok(id, json!({})),
            "tools/list" => Response::ok(id, self.list()),
            "tools/call" => match self.invoke(request.params.as_ref()) {
                Ok(result) => Response::ok(id, result),
                Err(error) => Response::fail(id, error),
            },
            method => Response::fail(id, Error::method_not_found(method)),
        })
    }

    /// Run one tool by name. The surface the tests drive, and what
    /// `tools/call` is a wire around.
    pub fn call(&self, name: &str, arguments: Value) -> Result<Answer, Trouble> {
        let Some(tool) = self.tools.iter().find(|tool| tool.name == name) else {
            return Err(Trouble::Invalid(format!("no tool {name}")));
        };
        (tool.call)(self.project.as_ref(), Args::new(&arguments))
    }

    fn initialize(&self) -> Value {
        json!({
            "protocolVersion": proto::VERSION,
            // `listChanged` is a promise to send a notification, and this
            // server has nowhere to send one from until the door grows a
            // stream. Saying false is what keeps a client from waiting for it.
            "capabilities": { "tools": { "listChanged": false } },
            "serverInfo": { "name": NAME, "version": env!("CARGO_PKG_VERSION") },
            "instructions": INSTRUCTIONS,
        })
    }

    fn list(&self) -> Value {
        json!({
            "tools": self
                .tools
                .iter()
                .map(|tool| json!({
                    "name": tool.name,
                    "description": tool.description,
                    "inputSchema": (tool.schema)(),
                }))
                .collect::<Vec<_>>(),
        })
    }

    /// `tools/call`, and the split that makes the surface usable: a malformed
    /// call is a JSON-RPC error, and a call that was fine but got no for an
    /// answer is a result carrying `isError` — which the model sees and can do
    /// something about.
    fn invoke(&self, params: Option<&Value>) -> Result<Value, Error> {
        let params = params.ok_or_else(|| Error::invalid_params("no params"))?;
        let name = params
            .get("name")
            .and_then(Value::as_str)
            .ok_or_else(|| Error::invalid_params("name is required, as a string"))?;
        let arguments = params
            .get("arguments")
            .cloned()
            .unwrap_or_else(|| json!({}));
        match self.call(name, arguments) {
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
