//! The inference-only boundary must not interpret function calls or consume errors.
use codex_api::AuthProvider;
use codex_api::Provider;
use codex_api::ResponsesClient;
use codex_api::ResponsesOptions;
use codex_client::HttpTransport;
use codex_client::Request;
use codex_client::Response;
use codex_client::StreamResponse;
use codex_client::TransportError;
use futures::StreamExt;
use http::HeaderMap;
use http::StatusCode;
use pretty_assertions::assert_eq;
use std::sync::Arc;
use std::time::Duration;

struct ByteTransport;
impl HttpTransport for ByteTransport {
    async fn execute(&self, _: Request) -> Result<Response, TransportError> {
        panic!("a prepared stream must not use execute")
    }
    async fn stream(&self, request: Request) -> Result<StreamResponse, TransportError> {
        assert_eq!(request.headers["session-id"], "caller-session");
        assert_eq!(request.headers["x-client-request-id"], "native-thread");
        Ok(StreamResponse {
            status: StatusCode::OK,
            headers: HeaderMap::new(),
            bytes: Box::pin(futures::stream::iter([
                Ok(bytes::Bytes::from_static(
                    b": comment\r\nid: opaque\r\ndata: {\"type\":\"function_call\",\"text\":\"\xe4",
                )),
                Ok(bytes::Bytes::from_static(
                    b"\xbd\xa0\"}\r\n\r\ndata: not-json\n\n",
                )),
                Err(TransportError::Network("interrupted wire".into())),
            ])),
        })
    }
}
struct NoAuth;
impl AuthProvider for NoAuth {
    fn add_auth_headers(&self, _: &mut HeaderMap) {}
}

#[tokio::test]
async fn prepared_bytes_preserve_split_utf8_opaque_events_and_transport_failure()
-> anyhow::Result<()> {
    let client = ResponsesClient::new(
        ByteTransport,
        Provider {
            name: "fixture".into(),
            base_url: "https://example.invalid/v1".into(),
            query_params: None,
            headers: HeaderMap::new(),
            retry: codex_api::RetryConfig {
                max_attempts: 0,
                base_delay: Duration::ZERO,
                retry_429: false,
                retry_5xx: false,
                retry_transport: false,
            },
            stream_idle_timeout: Duration::from_millis(/*millis*/ 1),
        },
        Arc::new(NoAuth),
    );
    let response = client
        .stream_prepared_body(
            serde_json::json!({"model":"fixture","stream":true}),
            ResponsesOptions {
                session_id: Some("caller-session".into()),
                thread_id: Some("native-thread".into()),
                ..Default::default()
            },
        )
        .await?;
    let chunks: Vec<_> = response
        .bytes
        .map(|chunk| {
            chunk
                .map(|bytes| bytes.to_vec())
                .map_err(|error| error.to_string())
        })
        .collect()
        .await;
    assert_eq!(
        chunks,
        vec![
            Ok(
                b": comment\r\nid: opaque\r\ndata: {\"type\":\"function_call\",\"text\":\"\xe4"
                    .to_vec()
            ),
            Ok(b"\xbd\xa0\"}\r\n\r\ndata: not-json\n\n".to_vec()),
            Err("network error: interrupted wire".into()),
        ]
    );
    Ok(())
}
