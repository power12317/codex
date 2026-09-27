use crate::ApiError;
use crate::safety_buffering::treatment_from_headers;
use codex_client::StreamResponse;
use http::HeaderMap;
use http::StatusCode;
use serde_json::Value;
use tokio::sync::mpsc;

/// Lossless Responses events after the official SSE and response processing path.
pub struct RawResponseStream {
    pub status: StatusCode,
    pub headers: HeaderMap,
    pub events: mpsc::Receiver<Result<Value, ApiError>>,
}

pub(crate) fn raw_response_stream(response: StreamResponse) -> RawResponseStream {
    let treatment = treatment_from_headers(&response.headers).unwrap_or_default();
    let (raw_tx, events) = mpsc::channel(/*buffer*/ 1);
    let (derived_tx, mut derived_rx) = mpsc::channel(/*buffer*/ 1);
    let error_tx = raw_tx.clone();
    tokio::spawn(async move {
        while let Some(event) = derived_rx.recv().await {
            if let Err(error) = event {
                let _ = error_tx.send(Err(error)).await;
                break;
            }
        }
    });
    tokio::spawn(super::responses::process_sse_with_treatment(
        response.bytes,
        derived_tx,
        /*idle_timeout*/ None,
        /*telemetry*/ None,
        treatment,
        Some(raw_tx),
    ));
    RawResponseStream {
        status: response.status,
        headers: response.headers,
        events,
    }
}

#[cfg(test)]
#[path = "raw_responses_tests.rs"]
mod tests;
