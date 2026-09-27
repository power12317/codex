//! Opt-in CPA IPC v1. No request may enter the agent/tool execution pipeline.
use crate::JsonSchema;
use crate::TS;
use serde::Deserialize;
use serde::Serialize;
use serde_json::Value;
use std::collections::BTreeMap;
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export_to = "v2/")]
pub struct CpaCapabilitiesReadParams {}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export_to = "v2/")]
pub struct CpaCapabilitiesReadResponse {
    pub protocol_version: u32,
    pub runtime_version: String,
    pub upstream_revision: String,
    pub credential_id: String,
    pub account_id: Option<String>,
    pub auth_mode: Option<String>,
    pub execution_mode: String,
    pub raw_events: bool,
    pub operations: Vec<String>,
    pub persistent_sessions: bool,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export_to = "v2/")]
pub struct CpaInferenceStartParams {
    pub request_id: String,
    pub credential_id: String,
    pub account_id: String,
    pub operation: String,
    pub source_format: String,
    pub session_id: String,
    pub request: serde_json::Map<String, Value>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export_to = "v2/")]
pub struct CpaInferenceStartResponse {
    pub request_id: String,
    pub status_code: u16,
    pub headers: BTreeMap<String, Vec<String>>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export_to = "v2/")]
pub struct CpaInferenceCancelParams {
    pub request_id: String,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export_to = "v2/")]
pub struct CpaInferenceCancelResponse {}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export_to = "v2/")]
pub struct CpaInferenceEventNotification {
    pub request_id: String,
    pub event: Value,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export_to = "v2/")]
pub struct CpaInferenceCompletedNotification {
    pub request_id: String,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export_to = "v2/")]
pub struct CpaInferenceErrorNotification {
    pub request_id: String,
    pub http_status: u16,
    pub message: String,
}
