//! One tool: what it is called, what it takes, and what it does.
//!
//! A pair of fn pointers rather than a boxed closure, so a tool set is a
//! `static` slice and mounting one costs nothing at all — see
//! [`crate::Server::mount`].

use serde_json::Value;
use std::path::Path;

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
    ///
    /// Takes whether the caller is already bound to a project: a session is
    /// opened in one, so the argument that names it is left out of the schema
    /// entirely rather than asked for and ignored.
    pub schema: fn(bound: bool) -> Value,
    /// Whether calling this changes the project. What the write switch reads,
    /// and a property of the tool rather than a second list somewhere that
    /// could disagree with it.
    pub writes: bool,
    pub call: fn(Args<'_>) -> Outcome,
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

/// The `arguments` object of a `tools/call`, and the project the caller is
/// working in when it has one.
///
/// Every argument this surface takes is a required string — an address, or a
/// line of text. The project is the exception, and the reason is that it is
/// not really an argument: a session is opened in a directory and the client
/// was told which, so asking its model to repeat it back is asking it to guess
/// at something already known.
pub struct Args<'a> {
    arguments: &'a Value,
    at: Option<&'a Path>,
}

impl<'a> Args<'a> {
    pub fn new(arguments: &'a Value, at: Option<&'a Path>) -> Self {
        Self { arguments, at }
    }

    /// The project the caller is bound to, if it is bound to one.
    pub fn at(&self) -> Option<&'a Path> {
        self.at
    }

    pub fn text(&self, name: &str) -> Result<&'a str, Trouble> {
        match self.arguments.get(name).and_then(Value::as_str) {
            Some(text) => Ok(text),
            None => Err(Trouble::Invalid(format!("{name} is required, as a string"))),
        }
    }

    pub fn maybe(&self, name: &str) -> Option<&'a str> {
        self.arguments.get(name).and_then(Value::as_str)
    }
}
