use super::*;
use crate::outgoing_message::OutgoingEnvelope;
use crate::outgoing_message::OutgoingMessage;
use axum::body::Bytes;
use pretty_assertions::assert_eq;
use tokio::sync::mpsc;

#[tokio::test]
async fn body_tap_preserves_comments_ids_utf8_chunks_and_midstream_errors() {
    let body = ": keepalive\r\nid: evt-1\r\nevent: response.output_text.delta\r\ndata: {\"delta\":\"你好\"}\r\n\r\n";
    let split = body.find('你').unwrap() + 1;
    let chunks = [
        Bytes::copy_from_slice(&body.as_bytes()[..split]),
        Bytes::copy_from_slice(&body.as_bytes()[split..]),
    ];
    for failure in [None, Some("fixture connection ended")] {
        let (tx, mut rx) = mpsc::channel(/*buffer*/ 4);
        let outgoing = Arc::new(OutgoingMessageSender::new(
            tx,
            codex_analytics::AnalyticsEventsClient::disabled(),
        ));
        let drain = tokio::spawn(async move {
            let mut logged = Vec::new();
            while let Some(envelope) = rx.recv().await {
                let OutgoingEnvelope::ToConnection {
                    connection_id,
                    message,
                    write_complete_tx,
                } = envelope
                else {
                    panic!("targeted log expected")
                };
                assert_eq!(connection_id, ConnectionId(42));
                let OutgoingMessage::AppServerNotification(envelope) = message else {
                    panic!("notification expected")
                };
                let ServerNotification::CpaInferenceUpstream(notification) = envelope.notification
                else {
                    panic!("upstream log expected")
                };
                logged.push(notification);
                write_complete_tx.unwrap().send(()).unwrap();
            }
            logged
        });
        let mut original: Vec<_> = chunks.iter().cloned().map(Ok).collect();
        if let Some(message) = failure {
            original.push(Err(TransportError::Network(message.into())));
        }
        let observed: Vec<_> = tap_body(
            futures::stream::iter(original).boxed(),
            outgoing,
            ConnectionId(42),
            "r".into(),
        )
        .map(|chunk| chunk.map_err(|err| err.to_string()))
        .collect()
        .await;
        let mut expected: Vec<_> = chunks.iter().cloned().map(Ok).collect();
        let mut expected_logs: Vec<_> = chunks
            .iter()
            .map(|bytes| CpaInferenceUpstreamNotification::Body {
                request_id: "r".into(),
                body_base64: base64::engine::general_purpose::STANDARD.encode(bytes),
            })
            .collect();
        if let Some(message) = failure {
            expected.push(Err(format!("network error: {message}")));
            expected_logs.push(CpaInferenceUpstreamNotification::Error {
                request_id: "r".into(),
                message: format!("network error: {message}"),
            });
        }
        assert_eq!(observed, expected);
        assert_eq!(drain.await.unwrap(), expected_logs);
    }
}
