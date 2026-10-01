use super::*;
use bytes::Bytes;
use codex_client::TransportError;
use http::HeaderMap;
use http::StatusCode;
use pretty_assertions::assert_eq;

#[tokio::test]
async fn raw_turn_state_keeps_first_valid_value_without_changing_bytes() {
    for newline in ["\n", "\r\n", "\r"] {
        for header in [None, Some("header-state"), Some("")] {
            let mut headers = HeaderMap::new();
            if let Some(header) = header {
                headers.insert(
                    X_CODEX_TURN_STATE_HEADER,
                    HeaderValue::from_str(header).unwrap(),
                );
            }
            let body = [
                ": preserve me".to_owned(),
                "data: not-json".to_owned(), String::new(),
                "data: {\"type\":\"response.metadata\",\"headers\":{\"x-codex-turn-state\":\"\"}}".to_owned(), String::new(),
                "data: {\"type\":\"response.metadata\",\"headers\":{\"X-Codex-Turn-State\":\"event-state\"}}".to_owned(), String::new(),
                "data: {\"type\":\"response.metadata\",\"headers\":{\"x-codex-turn-state\":\"ignored-state\"}}".to_owned(), String::new(), String::new(),
            ].join(newline);
            let chunks: Vec<_> = body
                .as_bytes()
                .chunks(/*chunk_size*/ 1)
                .map(Bytes::copy_from_slice)
                .map(Ok)
                .collect();
            let state = Arc::new(OnceLock::new());
            let mut response = observe(
                StreamResponse {
                    status: StatusCode::OK,
                    headers,
                    bytes: futures::stream::iter(chunks).boxed(),
                },
                state.clone(),
            );
            let mut actual = Vec::new();
            while let Some(chunk) = response.bytes.next().await {
                actual.extend(chunk.unwrap());
            }
            assert_eq!(
                (actual, state.get().map(String::as_str)),
                (
                    body.into_bytes(),
                    Some(
                        header
                            .filter(|value| !value.is_empty())
                            .unwrap_or("event-state")
                    )
                )
            );
        }
    }
}

#[tokio::test]
async fn raw_turn_state_preserves_transport_failure() {
    let mut response = observe(
        StreamResponse {
            status: StatusCode::OK,
            headers: HeaderMap::new(),
            bytes: futures::stream::iter([Err(TransportError::Network("broken stream".into()))])
                .boxed(),
        },
        Arc::new(OnceLock::new()),
    );
    assert!(
        matches!(response.bytes.next().await, Some(Err(TransportError::Network(message))) if message == "broken stream")
    );
}
