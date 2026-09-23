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
    /// Whether calling this takes an entry off the disk for good.
    ///
    /// Apart from [`Self::writes`] because the two answer different questions.
    /// Writing is the ordinary work of an agent and a project keeps its
    /// history of it; a deletion leaves nothing to read back. Every tool that
    /// deletes also writes, so the delete switch is the narrower of the two.
    pub deletes: bool,
    pub call: fn(Args<'_>) -> Outcome,
}

/// One argument a tool takes: the key it arrives under, and the line the model
/// reads to fill it in.
///
/// Named as a const and used by both sides — the schema that declares it and
/// the handler that reads it back. They used to be two string literals in two
/// files, which compiled just as happily when one of them was renamed and the
/// other was not.
#[derive(Clone, Copy)]
pub struct Arg {
    pub name: &'static str,
    pub about: &'static str,
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
/// A bound session supplies its project separately from the tool arguments.
pub struct Args<'a> {
    arguments: &'a Value,
    at: Option<&'a Path>,
    session: Option<&'a str>,
}

impl<'a> Args<'a> {
    pub fn new(arguments: &'a Value, at: Option<&'a Path>) -> Self {
        Self {
            arguments,
            at,
            session: None,
        }
    }

    /// The same, from the session filed under `session` — its record id.
    pub fn from_session(mut self, session: Option<&'a str>) -> Self {
        self.session = session;
        self
    }

    /// The record id of the session making the call, when the caller is one.
    pub fn session(&self) -> Option<&'a str> {
        self.session
    }

    /// The project the caller is bound to, if it is bound to one.
    pub fn at(&self) -> Option<&'a Path> {
        self.at
    }

    pub fn text(&self, arg: Arg) -> Result<&'a str, Trouble> {
        match self.arguments.get(arg.name).and_then(Value::as_str) {
            Some(text) => Ok(text),
            None => Err(Trouble::Invalid(format!(
                "{} is required, as a string",
                arg.name
            ))),
        }
    }

    /// One or several, for an argument a caller may say twice. A bare string
    /// is one of them: a client with a single thing to name should not have to
    /// wrap it, and a tool that takes a list takes a list of one.
    pub fn list(&self, arg: Arg) -> Result<Vec<&'a str>, Trouble> {
        let wrong = || {
            Trouble::Invalid(format!(
                "{} is required, as a string or a list of strings",
                arg.name
            ))
        };
        match self.arguments.get(arg.name) {
            Some(Value::String(one)) => Ok(vec![one.as_str()]),
            Some(Value::Array(many)) if !many.is_empty() => many
                .iter()
                .map(|value| value.as_str().ok_or_else(wrong))
                .collect(),
            _ => Err(wrong()),
        }
    }

    pub fn maybe(&self, arg: Arg) -> Option<&'a str> {
        self.arguments.get(arg.name).and_then(Value::as_str)
    }

    pub fn boolean(&self, arg: Arg, default: bool) -> Result<bool, Trouble> {
        match self.arguments.get(arg.name) {
            None => Ok(default),
            Some(Value::Bool(value)) => Ok(*value),
            Some(_) => Err(Trouble::Invalid(format!("{} must be a boolean", arg.name))),
        }
    }
}
