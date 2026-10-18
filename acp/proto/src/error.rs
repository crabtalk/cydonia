//! JSON-RPC error type and the codes ACP defines over it.

use serde::{Deserialize, Serialize};
use std::fmt::{self, Display};

/// Malformed JSON.
pub const PARSE_ERROR: i32 = -32700;
/// Not a valid request object.
pub const INVALID_REQUEST: i32 = -32600;
/// The method is not implemented by this side.
pub const METHOD_NOT_FOUND: i32 = -32601;
/// Params did not match the method's schema.
pub const INVALID_PARAMS: i32 = -32602;
/// The handler failed.
pub const INTERNAL_ERROR: i32 = -32603;
/// The request was cancelled before it completed.
pub const REQUEST_CANCELLED: i32 = -32800;
/// The agent needs `authenticate` before it can serve this.
pub const AUTH_REQUIRED: i32 = -32000;
/// The requested resource does not exist.
pub const RESOURCE_NOT_FOUND: i32 = -32002;

/// A JSON-RPC error, as it appears on the wire.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Error {
    pub code: i32,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

impl Error {
    pub fn new(code: i32, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            data: None,
        }
    }

    /// Attach diagnostic detail to an error.
    #[must_use]
    pub fn data(mut self, data: impl Into<serde_json::Value>) -> Self {
        self.data = Some(data.into());
        self
    }

    pub fn parse_error() -> Self {
        Self::new(PARSE_ERROR, "Parse error")
    }

    pub fn invalid_request() -> Self {
        Self::new(INVALID_REQUEST, "Invalid request")
    }

    pub fn method_not_found() -> Self {
        Self::new(METHOD_NOT_FOUND, "Method not found")
    }

    pub fn invalid_params() -> Self {
        Self::new(INVALID_PARAMS, "Invalid params")
    }

    pub fn internal_error() -> Self {
        Self::new(INTERNAL_ERROR, "Internal error")
    }

    pub fn request_cancelled() -> Self {
        Self::new(REQUEST_CANCELLED, "Request cancelled")
    }

    pub fn auth_required() -> Self {
        Self::new(AUTH_REQUIRED, "Authentication required")
    }

    pub fn resource_not_found(uri: impl Into<String>) -> Self {
        Self::new(RESOURCE_NOT_FOUND, "Resource not found")
            .data(serde_json::json!({ "uri": uri.into() }))
    }

    pub fn is_auth_required(&self) -> bool {
        self.code == AUTH_REQUIRED
    }
}

impl Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} ({})", self.message, self.code)?;
        if let Some(data) = &self.data {
            write!(f, ": {data}")?;
        }
        Ok(())
    }
}

impl std::error::Error for Error {}

impl From<serde_json::Error> for Error {
    fn from(e: serde_json::Error) -> Self {
        Self::parse_error().data(e.to_string())
    }
}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Self::internal_error().data(e.to_string())
    }
}

/// Every fallible operation in this crate reports a wire-shaped error.
pub type Result<T, E = Error> = std::result::Result<T, E>;
