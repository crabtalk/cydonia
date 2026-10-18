//! Tool calls and the incremental updates an agent reports for them.

use crate::{ContentBlock, Meta, TerminalId, ToolCallId};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, path::PathBuf};

/// A tool invocation the agent is reporting to the client.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolCall {
    pub tool_call_id: ToolCallId,
    pub title: String,
    /// The tool's own name, where `title` is what the user is shown.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "ToolKind::is_default")]
    pub kind: ToolKind,
    #[serde(default, skip_serializing_if = "ToolCallStatus::is_default")]
    pub status: ToolCallStatus,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub content: Vec<ToolCallContent>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub locations: Vec<ToolCallLocation>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub raw_input: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub raw_output: Option<serde_json::Value>,
    #[serde(default, rename = "_meta", skip_serializing_if = "Option::is_none")]
    pub meta: Option<Meta>,
}

impl ToolCall {
    pub fn new(tool_call_id: impl Into<ToolCallId>, title: impl Into<String>) -> Self {
        Self {
            tool_call_id: tool_call_id.into(),
            title: title.into(),
            name: None,
            kind: ToolKind::default(),
            status: ToolCallStatus::default(),
            content: Vec::new(),
            locations: Vec::new(),
            raw_input: None,
            raw_output: None,
            meta: None,
        }
    }
}

/// A partial update to an in-flight tool call; absent fields are unchanged.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolCallUpdate {
    pub tool_call_id: ToolCallId,
    #[serde(flatten)]
    pub fields: ToolCallUpdateFields,
    #[serde(default, rename = "_meta", skip_serializing_if = "Option::is_none")]
    pub meta: Option<Meta>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolCallUpdateFields {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<ToolKind>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<ToolCallStatus>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content: Option<Vec<ToolCallContent>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub locations: Option<Vec<ToolCallLocation>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub raw_input: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub raw_output: Option<serde_json::Value>,
}

impl ToolCallUpdate {
    /// Fold this update into an existing tool call.
    pub fn apply(self, target: &mut ToolCall) {
        let ToolCallUpdateFields {
            kind,
            status,
            title,
            name,
            content,
            locations,
            raw_input,
            raw_output,
        } = self.fields;
        if let Some(kind) = kind {
            target.kind = kind;
        }
        if let Some(status) = status {
            target.status = status;
        }
        if let Some(title) = title {
            target.title = title;
        }
        if name.is_some() {
            target.name = name;
        }
        if let Some(content) = content {
            target.content = content;
        }
        if let Some(locations) = locations {
            target.locations = locations;
        }
        if raw_input.is_some() {
            target.raw_input = raw_input;
        }
        if raw_output.is_some() {
            target.raw_output = raw_output;
        }
    }
}

impl From<ToolCall> for ToolCallUpdate {
    fn from(call: ToolCall) -> Self {
        Self {
            tool_call_id: call.tool_call_id,
            fields: ToolCallUpdateFields {
                kind: Some(call.kind),
                status: Some(call.status),
                title: Some(call.title),
                name: call.name,
                content: Some(call.content),
                locations: Some(call.locations),
                raw_input: call.raw_input,
                raw_output: call.raw_output,
            },
            meta: None,
        }
    }
}

/// What a tool does, so the client can pick an icon and a phrasing.
///
/// `Other` is the `#[serde(other)]` catch-all: a kind added in a later
/// revision deserializes instead of failing the whole update.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolKind {
    Read,
    Edit,
    Delete,
    Move,
    Search,
    Execute,
    Think,
    Fetch,
    SwitchMode,
    #[default]
    #[serde(other)]
    Other,
}

impl ToolKind {
    fn is_default(&self) -> bool {
        matches!(self, Self::Other)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolCallStatus {
    #[default]
    Pending,
    InProgress,
    Completed,
    Failed,
    #[serde(untagged)]
    Other(String),
}

impl ToolCallStatus {
    fn is_default(&self) -> bool {
        matches!(self, Self::Pending)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ToolCallContent {
    Content {
        content: ContentBlock,
    },
    Diff(Diff),
    // `rename_all` on the container renames variants, not their fields.
    #[serde(rename_all = "camelCase")]
    Terminal {
        terminal_id: TerminalId,
    },
    #[serde(untagged)]
    Other(OtherToolCallContent),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OtherToolCallContent {
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(flatten)]
    pub fields: BTreeMap<String, serde_json::Value>,
}

impl<T: Into<ContentBlock>> From<T> for ToolCallContent {
    fn from(content: T) -> Self {
        Self::Content {
            content: content.into(),
        }
    }
}

/// A proposed edit, rendered by the client as a diff.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Diff {
    pub path: PathBuf,
    pub new_text: String,
    /// Absent for a newly created file.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub old_text: Option<String>,
    #[serde(default, rename = "_meta", skip_serializing_if = "Option::is_none")]
    pub meta: Option<Meta>,
}

/// A file the tool call touches, so the client can follow along.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolCallLocation {
    pub path: PathBuf,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line: Option<u32>,
    #[serde(default, rename = "_meta", skip_serializing_if = "Option::is_none")]
    pub meta: Option<Meta>,
}
