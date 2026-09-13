//! The wire: JSON-RPC 2.0, and the envelope MCP rides in it.
//!
//! Hand-written because this is all of it — an envelope, five error codes and
//! the handful of methods [`crate::Server`] answers. What is not hand-written
//! is the HTTP underneath, which is where the bugs that are worth paying a
//! dependency to avoid actually live.
//!
//! A batch is not a shape this answers. Batching went out of the protocol with
//! the revision below, and a server that accepts an array of calls has to
//! decide what a half-failed batch means.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// The revision this speaks. Answered as-is whatever the client asked for:
/// where the versions differ the server names one it supports, and it supports
/// this one.
pub const VERSION: &str = "2025-06-18";

const JSONRPC: &str = "2.0";

pub const INVALID_REQUEST: i64 = -32600;
pub const METHOD_NOT_FOUND: i64 = -32601;
pub const INVALID_PARAMS: i64 = -32602;
pub const INTERNAL_ERROR: i64 = -32603;

/// One call off the wire.
#[derive(Debug, Clone, Deserialize)]
pub struct Request {
    /// Echoed back untouched. A string or a number on the wire, and nothing
    /// here reads it, so it is carried as it arrived rather than re-typed into
    /// something that would have to be turned back.
    ///
    /// Absent is what makes a frame a notification — see [`crate::Server::handle`].
    #[serde(default)]
    pub id: Option<Value>,
    pub method: String,
    #[serde(default)]
    pub params: Option<Value>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Response {
    pub jsonrpc: &'static str,
    pub id: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<Error>,
}

impl Response {
    pub fn ok(id: Value, result: Value) -> Self {
        Self {
            jsonrpc: JSONRPC,
            id,
            result: Some(result),
            error: None,
        }
    }

    pub fn fail(id: Value, error: Error) -> Self {
        Self {
            jsonrpc: JSONRPC,
            id,
            result: None,
            error: Some(error),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Error {
    pub code: i64,
    pub message: String,
}

impl Error {
    pub fn new(code: i64, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }

    /// A method this server does not serve. Named rather than refused with a
    /// bare code so the caller can see which one it asked for.
    pub fn method_not_found(method: &str) -> Self {
        Self::new(METHOD_NOT_FOUND, format!("no method {method}"))
    }

    pub fn invalid_params(message: impl Into<String>) -> Self {
        Self::new(INVALID_PARAMS, message)
    }
}
