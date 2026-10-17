//! `session/request_permission` — the agent asking the user to authorize a tool call.

use crate::{Meta, PermissionOptionId, SessionId, ToolCallUpdate};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RequestPermissionRequest {
    pub session_id: SessionId,
    pub tool_call: ToolCallUpdate,
    pub options: Vec<PermissionOption>,
    #[serde(default, rename = "_meta", skip_serializing_if = "Option::is_none")]
    pub meta: Option<Meta>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PermissionOption {
    pub option_id: PermissionOptionId,
    pub name: String,
    pub kind: PermissionOptionKind,
    #[serde(default, rename = "_meta", skip_serializing_if = "Option::is_none")]
    pub meta: Option<Meta>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PermissionOptionKind {
    AllowOnce,
    AllowAlways,
    RejectOnce,
    RejectAlways,
    #[serde(untagged)]
    Other(String),
}

/// Wraps [`RequestPermissionOutcome`] because a JSON-RPC result must be an object.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RequestPermissionResponse {
    pub outcome: RequestPermissionOutcome,
    #[serde(default, rename = "_meta", skip_serializing_if = "Option::is_none")]
    pub meta: Option<Meta>,
}

impl RequestPermissionResponse {
    pub fn selected(option_id: impl Into<PermissionOptionId>) -> Self {
        Self {
            outcome: RequestPermissionOutcome::Selected {
                option_id: option_id.into(),
            },
            meta: None,
        }
    }

    /// The answer when the turn is cancelled before the user chose.
    pub fn cancelled() -> Self {
        Self {
            outcome: RequestPermissionOutcome::Cancelled,
            meta: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum RequestPermissionOutcome {
    Cancelled,
    #[serde(rename_all = "camelCase")]
    Selected {
        option_id: PermissionOptionId,
    },
}
