//! The protocol's string-newtype identifiers.

use serde::{Deserialize, Serialize};
use std::{fmt, sync::Arc};

/// `Arc<str>` because ids are cloned onto every message about the thing they name.
macro_rules! id {
    ($(#[$doc:meta] $name:ident),* $(,)?) => {$(
        #[$doc]
        #[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(pub Arc<str>);

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&self.0)
            }
        }

        impl<T: Into<Arc<str>>> From<T> for $name {
            fn from(value: T) -> Self {
                Self(value.into())
            }
        }

        impl std::ops::Deref for $name {
            type Target = str;

            fn deref(&self) -> &str {
                &self.0
            }
        }
    )*};
}

id! {
    /// Identifies a conversation with an agent.
    SessionId,
    /// Identifies one tool invocation within a session.
    ToolCallId,
    /// Identifies a terminal the client runs on the agent's behalf.
    TerminalId,
    /// Identifies an authentication method the agent offers.
    AuthMethodId,
    /// Identifies one of the agent's operating modes.
    SessionModeId,
    /// Identifies a per-session configuration option.
    SessionConfigId,
    /// Identifies one selectable value of a config option.
    SessionConfigValueId,
    /// Groups related config option values.
    SessionConfigGroupId,
    /// Identifies one choice offered in a permission request.
    PermissionOptionId,
    /// Groups content chunks belonging to the same assistant message.
    MessageId,
    /// Identifies an in-flight elicitation.
    ElicitationId,
    /// Identifies one of the agent's plans within a session.
    PlanId,
    /// Identifies an LLM provider the agent can be pointed at.
    ProviderId,
    /// Names an MCP server the agent reaches through the client.
    McpServerAcpId,
    /// Identifies one live MCP-over-ACP connection the client holds.
    McpConnectionId,
    /// Identifies one suggestion within a next-edit response.
    NesSuggestionId,
}

/// A JSON-RPC request id. Any shape is legal; peers echo back whatever they
/// were sent. [`Null`](RequestId::Null) is the id of an error about a frame
/// too malformed to have one.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(untagged)]
pub enum RequestId {
    Num(i64),
    Str(String),
    Null,
}

impl fmt::Display for RequestId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Num(n) => write!(f, "{n}"),
            Self::Str(s) => f.write_str(s),
            Self::Null => f.write_str("null"),
        }
    }
}

impl From<i64> for RequestId {
    fn from(n: i64) -> Self {
        Self::Num(n)
    }
}

impl From<String> for RequestId {
    fn from(s: String) -> Self {
        Self::Str(s)
    }
}
