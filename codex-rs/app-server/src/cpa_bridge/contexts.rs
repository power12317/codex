//! Worker-local turn contexts; the lock never spans network or inference work.
use codex_core::InferenceIdentity;
use codex_core::InferenceSession;
use std::collections::HashMap;
use std::sync::Arc;

type Key = (String, Option<String>, Option<String>, String);

#[derive(Default)]
pub(super) struct Contexts {
    turns: HashMap<Key, Arc<InferenceSession>>,
}

impl Contexts {
    pub(super) fn get(
        &mut self,
        bridge: &super::CpaBridge,
        params: &codex_app_server_protocol::CpaInferenceStartParams,
    ) -> anyhow::Result<Arc<InferenceSession>> {
        let source = InferenceIdentity::from_request(&params.request);
        let key = source.turn_id.as_ref().map(|turn| {
            (
                params.session_id.clone(),
                source.session_id.clone(),
                source.thread_id.clone(),
                turn.clone(),
            )
        });
        if let Some(key) = &key
            && let Some(session) = self.turns.get_mut(key)
        {
            return Ok(session.clone());
        }
        let scope = serde_json::to_string(&(&params.credential_id, &params.session_id))?;
        let session = Arc::new(InferenceSession::new(
            &bridge.config,
            bridge.auth.clone(),
            &bridge.installation_id,
            &scope,
            &source,
        )?);
        // Without a source turn ID, each RPC is isolated, including from prior calls.
        if let Some(key) = key {
            self.turns.insert(key, session.clone());
        }
        Ok(session)
    }
}
