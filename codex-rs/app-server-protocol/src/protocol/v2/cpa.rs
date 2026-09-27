//! Opt-in CPA IPC v2. No request may enter the agent/tool execution pipeline.
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
    pub credential_file: String,
    pub auth_owner: Option<String>,
    #[serde(rename = "manualOAuth")]
    #[ts(rename = "manualOAuth")]
    pub manual_oauth: bool,
    pub upstream_logs: bool,
    pub upstream_body_logs: bool,
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
    pub body: Option<String>,
    pub headers: Option<BTreeMap<String, Vec<String>>>,
    pub request_id: String,
    pub http_status: u16,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export_to = "v2/")]
pub struct CpaCredentialReloadParams {}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export_to = "v2/")]
pub struct CpaAuthLoginStartParams {}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export_to = "v2/")]
pub struct CpaAuthLoginStartResponse {
    pub login_id: String,
    pub auth_url: String,
    pub state: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export_to = "v2/")]
pub struct CpaAuthLoginCallbackParams {
    pub login_id: String,
    pub redirect_url: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export_to = "v2/")]
pub struct CpaAuthLoginStatusParams {
    pub login_id: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export_to = "v2/")]
pub struct CpaAuthLoginStatusResponse {
    pub status: String,
    pub error: Option<String>,
}

/// Actual inference transport exchanges, including attempts made during auth recovery.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
#[ts(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    export_to = "v2/"
)]
pub enum CpaInferenceUpstreamNotification {
    Request {
        request_id: String,
        url: String,
        method: String,
        headers: BTreeMap<String, Vec<String>>,
        body: String,
        access_token_sha256: Option<String>,
        #[serde(rename = "oaiLbNode")]
        #[ts(rename = "oaiLbNode")]
        oai_lb_node: Option<String>,
    },
    Response {
        request_id: String,
        status_code: u16,
        headers: BTreeMap<String, Vec<String>>,
        body: Option<String>,
        #[serde(rename = "oaiLbNode")]
        #[ts(rename = "oaiLbNode")]
        oai_lb_node: Option<String>,
    },
    Body {
        request_id: String,
        body_base64: String,
    },
    Error {
        request_id: String,
        message: String,
    },
}
