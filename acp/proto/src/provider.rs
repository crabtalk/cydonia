//! Pointing the agent at a different LLM endpoint.

use crate::{Meta, ProviderId};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// The API dialect an endpoint speaks. `Other` is the untagged fallback, so a
/// dialect added later deserializes as itself instead of failing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LlmProtocol {
    Anthropic,
    #[serde(rename = "openai")]
    OpenAi,
    Azure,
    Vertex,
    Bedrock,
    #[serde(untagged)]
    Other(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderInfo {
    pub provider_id: ProviderId,
    pub supported: Vec<LlmProtocol>,
    /// The agent cannot run until this provider is configured.
    pub required: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub current: Option<ProviderCurrentConfig>,
    #[serde(default, rename = "_meta", skip_serializing_if = "Option::is_none")]
    pub meta: Option<Meta>,
}

/// What the provider is pointed at right now.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderCurrentConfig {
    pub api_type: LlmProtocol,
    pub base_url: String,
    #[serde(default, rename = "_meta", skip_serializing_if = "Option::is_none")]
    pub meta: Option<Meta>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ListProvidersRequest {
    #[serde(default, rename = "_meta", skip_serializing_if = "Option::is_none")]
    pub meta: Option<Meta>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListProvidersResponse {
    pub providers: Vec<ProviderInfo>,
    #[serde(default, rename = "_meta", skip_serializing_if = "Option::is_none")]
    pub meta: Option<Meta>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetProviderRequest {
    pub provider_id: ProviderId,
    pub api_type: LlmProtocol,
    pub base_url: String,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub headers: HashMap<String, String>,
    #[serde(default, rename = "_meta", skip_serializing_if = "Option::is_none")]
    pub meta: Option<Meta>,
}

impl SetProviderRequest {
    pub fn new(
        provider_id: impl Into<ProviderId>,
        api_type: LlmProtocol,
        base_url: impl Into<String>,
    ) -> Self {
        Self {
            provider_id: provider_id.into(),
            api_type,
            base_url: base_url.into(),
            headers: HashMap::new(),
            meta: None,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct SetProviderResponse {
    #[serde(default, rename = "_meta", skip_serializing_if = "Option::is_none")]
    pub meta: Option<Meta>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DisableProviderRequest {
    pub provider_id: ProviderId,
    #[serde(default, rename = "_meta", skip_serializing_if = "Option::is_none")]
    pub meta: Option<Meta>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct DisableProviderResponse {
    #[serde(default, rename = "_meta", skip_serializing_if = "Option::is_none")]
    pub meta: Option<Meta>,
}
