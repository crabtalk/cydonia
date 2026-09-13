//! One tool: what it is called, what it takes, and what it does.
//!
//! A pair of fn pointers rather than a boxed closure, so a tool set is a
//! `static` slice and mounting one costs nothing at all — see
//! [`crate::Server::mount`].

use artifact::project::Project;
use serde_json::Value;

pub struct Tool {
    /// What `tools/call` names. Flat and snake_cased, which is what every
    /// client accepts.
    pub name: &'static str,
    /// What the model reads to decide this is the one. Spent on every turn, so
    /// it is a sentence.
    pub description: &'static str,
    /// The JSON Schema of `arguments`, built when it is asked for. A `Value`
    /// held in a `static` would want a lock or a lazy, for a list that is
    /// rebuilt once per `tools/list`.
    pub schema: fn() -> Value,
    pub call: fn(&dyn Project, Args<'_>) -> Outcome,
}

pub type Outcome = Result<Answer, Trouble>;

/// What a tool answers with: the lines a model reads, and the shape a client
/// renders.
///
/// The text is not the JSON. A rendering a model can read beats a
/// serialisation it has to parse, and anything that wants fields has `data` —
/// which is the opposite way round from a server that puts its JSON in both.
pub struct Answer {
    pub text: String,
    pub data: Option<Value>,
}

impl Answer {
    pub fn said(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            data: None,
        }
    }

    pub fn with(mut self, data: Value) -> Self {
        self.data = Some(data);
        self
    }
}

/// The two ways a call does not work, which are not the same thing to whoever
/// made it.
pub enum Trouble {
    /// The call was malformed — an argument missing, a number where a string
    /// goes. A protocol error, because it is the client that is wrong and the
    /// model cannot fix its client by trying again.
    Invalid(String),
    /// The call was well formed and the answer is no. Handed back as a
    /// *result* rather than an error: "there is no ROAD-99, and here is what
    /// there is" is something a model can act on, and an error is something it
    /// can only give up on.
    Refused(String),
}

/// The `arguments` object of a `tools/call`, and the two questions anything
/// here asks of it. Every argument this surface takes is a required string —
/// an address, or a line of text.
pub struct Args<'a>(&'a Value);

impl<'a> Args<'a> {
    pub fn new(arguments: &'a Value) -> Self {
        Self(arguments)
    }

    pub fn text(&self, name: &str) -> Result<&'a str, Trouble> {
        match self.0.get(name).and_then(Value::as_str) {
            Some(text) => Ok(text),
            None => Err(Trouble::Invalid(format!("{name} is required, as a string"))),
        }
    }

    pub fn maybe(&self, name: &str) -> Option<&'a str> {
        self.0.get(name).and_then(Value::as_str)
    }
}
