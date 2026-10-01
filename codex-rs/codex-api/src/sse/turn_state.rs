//! Observe routing metadata while forwarding every original HTTP byte unchanged.
use super::ResponsesStreamEvent;
use super::responses::X_CODEX_TURN_STATE_HEADER;
use codex_client::StreamResponse;
use futures::StreamExt;
use http::HeaderValue;
use std::sync::Arc;
use std::sync::OnceLock;

pub(crate) fn observe(
    mut response: StreamResponse,
    state: Arc<OnceLock<String>>,
) -> StreamResponse {
    if let Some(value) = response
        .headers
        .get(X_CODEX_TURN_STATE_HEADER)
        .and_then(|value| value.to_str().ok())
    {
        remember(&state, value.to_owned());
    }
    let mut line = Vec::new();
    let mut data = Vec::new();
    let mut previous_cr = false;
    response.bytes = Box::pin(response.bytes.map(move |chunk| {
        if state.get().is_none()
            && let Ok(bytes) = &chunk
        {
            for &byte in bytes {
                if byte == b'\n' && previous_cr {
                    previous_cr = false;
                    continue;
                }
                previous_cr = byte == b'\r';
                if byte == b'\r' || byte == b'\n' {
                    if line.is_empty() {
                        if let Ok(event) = serde_json::from_slice::<ResponsesStreamEvent>(&data)
                            && let Some(value) = event.turn_state()
                        {
                            remember(&state, value);
                        }
                        data.clear();
                    } else if let Some(value) = line.strip_prefix(b"data:") {
                        data.extend_from_slice(value.strip_prefix(b" ").unwrap_or(value));
                        data.push(b'\n');
                    }
                    line.clear();
                } else {
                    line.push(byte);
                }
            }
        }
        chunk
    }));
    response
}

fn remember(state: &OnceLock<String>, value: String) {
    if !value.trim().is_empty() && HeaderValue::from_str(&value).is_ok() {
        let _ = state.set(value);
    }
}

#[cfg(test)]
#[path = "turn_state_tests.rs"]
mod tests;
