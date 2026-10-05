//! Native request identity and turn state for caller-owned inference.
use super::*;
use crate::responses_metadata::CodexResponsesRequestKind;
use serde::Serialize;
use serde_json::Map;
use serde_json::Value;

/// Business identities supplied by the caller, independent of worker authentication.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct InferenceIdentity {
    pub session_id: Option<String>,
    pub thread_id: Option<String>,
    pub turn_id: Option<String>,
    pub prompt_cache_key: Option<String>,
    pub parent_turn_id: Option<String>,
    pub root_turn_id: Option<String>,
}

impl InferenceIdentity {
    /// Prefer a source thread, then a source session, within the CPA RPC scope.
    /// Cache keys are routing hints and never define a conversation's identity.
    pub fn conversation_key(&self) -> Option<&str> {
        self.thread_id.as_deref().or(self.session_id.as_deref())
    }

    pub fn from_request(request: &Map<String, Value>) -> Self {
        let metadata = request.get("client_metadata").and_then(Value::as_object);
        let nested: Option<Value> = metadata
            .and_then(|metadata| metadata.get("x-codex-turn-metadata"))
            .or_else(|| request.get("x-codex-turn-metadata"))
            .and_then(Value::as_str)
            .and_then(|value| serde_json::from_str(value).ok());
        let read = |key| {
            [
                request.get(key),
                metadata.and_then(|metadata| metadata.get(key)),
                nested.as_ref().and_then(|nested| nested.get(key)),
            ]
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .find(|value| !value.is_empty())
            .map(str::to_owned)
        };
        Self {
            session_id: read("session_id"),
            thread_id: read("thread_id"),
            turn_id: read("turn_id"),
            prompt_cache_key: read("prompt_cache_key"),
            parent_turn_id: read("parent_turn_id"),
            root_turn_id: read("root_turn_id"),
        }
    }
}

#[cfg(test)]
#[path = "inference_session_tests.rs"]
mod tests;

/// A caller turn's native client and first upstream routing state. No tool runtime.
pub struct InferenceSession {
    pub(super) session: ModelClientSession,
    pub(super) metadata: CodexResponsesMetadata,
}

impl InferenceSession {
    pub fn new(
        config: &crate::config::Config,
        auth: Arc<AuthManager>,
        installation_id: &str,
        scope: &str,
        source: &InferenceIdentity,
    ) -> anyhow::Result<Self> {
        let namespace = Uuid::new_v5(&Uuid::NAMESPACE_OID, scope.as_bytes());
        anyhow::ensure!(
            source.turn_id.is_some(),
            "inference turn_id must be resolved"
        );
        let session_id = Uuid::new_v5(&namespace, &serde_json::to_vec(&source.conversation_key())?);
        let thread_id = session_id;
        let client = ModelClient::new(
            Some(auth),
            AgentIdentityAuthPolicy::JwtOnly,
            ThreadId::from_string(&thread_id.to_string())?,
            config.model_provider.clone(),
            SessionSource::Cli,
            "codex_cli_rs".into(),
            config.model_verbosity,
            /*content_item_kinds_enabled*/ false,
            /*reasoning_effort_override_enabled*/ false,
            /*enable_request_compression*/ false,
            /*include_timing_metrics*/ false,
            /*beta_features_header*/ None,
            /*concurrent_reasoning_summaries_enabled*/ false,
            /*attestation_provider*/ None,
            config.http_client_factory(),
            config.workspace_routing_context(),
            Vec::new(),
        );
        let mut metadata = CodexResponsesMetadata::new(
            installation_id.into(),
            session_id.to_string(),
            thread_id.to_string(),
            namespace.to_string(),
        );
        metadata.turn_id.clone_from(&source.turn_id);
        metadata.parent_turn_id.clone_from(&source.parent_turn_id);
        metadata.root_turn_id.clone_from(&source.root_turn_id);
        metadata.request_kind = Some(CodexResponsesRequestKind::Turn);
        Ok(Self {
            session: client.new_session(),
            metadata,
        })
    }

    pub fn turn_state(&self) -> Option<&str> {
        self.session.turn_state.get().map(String::as_str)
    }
}
