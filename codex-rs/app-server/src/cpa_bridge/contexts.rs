//! Bounded worker-local contexts. Synthetic turns have a fixed one-hour lifetime.
use codex_core::InferenceIdentity;
use codex_core::InferenceSession;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use std::time::Instant;

const TURN_TTL: Duration = Duration::from_secs(3600);
const MAX_CONTEXTS: usize = 1024;

#[derive(Hash, PartialEq, Eq)]
struct Key {
    scope: String,
    conversation: Option<String>,
    turn: Option<String>,
}

struct Entry<T> {
    created: Instant,
    last_used: Instant,
    value: Arc<T>,
}

pub(super) struct Cache<T> {
    turns: HashMap<Key, Entry<T>>,
}

impl<T> Default for Cache<T> {
    fn default() -> Self {
        Self {
            turns: HashMap::new(),
        }
    }
}

#[derive(Debug, thiserror::Error)]
#[error("Inference context capacity reached; retry after an existing context expires")]
pub(super) struct CapacityExceeded;

impl<T> Cache<T> {
    fn get_or_insert(
        &mut self,
        key: Key,
        now: Instant,
        create: impl FnOnce() -> anyhow::Result<T>,
    ) -> anyhow::Result<Arc<T>> {
        self.turns.retain(|key, entry| {
            let since = if key.turn.is_some() {
                entry.last_used
            } else {
                entry.created
            };
            now.duration_since(since) < TURN_TTL
        });
        if let Some(entry) = self.turns.get_mut(&key) {
            entry.last_used = now;
            return Ok(entry.value.clone());
        }
        // Never evict a fresh turn and silently lose its routing state under load.
        if self.turns.len() >= MAX_CONTEXTS {
            return Err(CapacityExceeded.into());
        }
        let value = Arc::new(create()?);
        self.turns.insert(
            key,
            Entry {
                created: now,
                last_used: now,
                value: value.clone(),
            },
        );
        Ok(value)
    }
}

pub(super) type Contexts = Cache<InferenceSession>;

impl Contexts {
    pub(super) fn get(
        &mut self,
        bridge: &super::CpaBridge,
        params: &codex_app_server_protocol::CpaInferenceStartParams,
    ) -> anyhow::Result<Arc<InferenceSession>> {
        let mut source = InferenceIdentity::from_request(&params.request);
        let key = Key {
            scope: params.session_id.clone(),
            conversation: source.conversation_key().map(str::to_owned),
            turn: source.turn_id.clone(),
        };
        self.get_or_insert(key, Instant::now(), || {
            source
                .turn_id
                .get_or_insert_with(|| uuid::Uuid::now_v7().to_string());
            let scope = serde_json::to_string(&(&params.credential_id, &params.session_id))?;
            InferenceSession::new(
                &bridge.config,
                bridge.auth.clone(),
                &bridge.installation_id,
                &scope,
                &source,
            )
        })
    }
}

#[cfg(test)]
#[path = "contexts_tests.rs"]
mod tests;
