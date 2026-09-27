use super::*;
use bytes::Bytes;
use futures::StreamExt;
use pretty_assertions::assert_eq;
use serde_json::json;

#[tokio::test]
async fn preserves_unknown_events_tool_calls_and_all_terminal_fields() {
    for terminal in [
        "response.completed",
        "response.incomplete",
        "response.failed",
    ] {
        let expected = vec![
            json!({"type":"response.created", "response":{"id":"r"}, "extension":42}),
            json!({"type":"future.event", "unknown":{"deep":[1,2,3]}, "delta":{"future":"non-string"}}),
            json!({"type":"response.output_item.done", "item":{"type":"function_call", "name":"shell", "call_id":"call", "arguments":"{\"command\":\"touch /tmp/never-execute\"}"}}),
            json!({"type":terminal,"response":{"id":"r","status":"incomplete","incomplete_details":{"reason":"max_output_tokens"},"usage":{"input_tokens":1,"output_tokens":2,"total_tokens":3,"future_tokens":4}}}),
        ];
        let body = expected
            .iter()
            .map(|v| format!("data: {v}\n\n"))
            .collect::<String>();
        // Exercise event framing across arbitrary byte boundaries through the official parser.
        let chunks: Vec<_> = body
            .as_bytes()
            .chunks(7)
            .map(Bytes::copy_from_slice)
            .map(Ok)
            .collect();
        let mut stream = raw_response_stream(StreamResponse {
            status: StatusCode::OK,
            headers: HeaderMap::new(),
            bytes: futures::stream::iter(chunks).boxed(),
        });
        let mut actual = Vec::new();
        while let Some(event) = stream.events.recv().await {
            actual.push(event.unwrap());
        }
        assert_eq!(actual, expected);
    }
}

#[tokio::test(start_paused = true)]
async fn no_idle_deadline_and_dropping_receiver_cancels_upstream() {
    let (tx, rx) = mpsc::channel(/*buffer*/ 1);
    let bytes = futures::stream::unfold(rx, |mut rx| async {
        rx.recv().await.map(|bytes| (bytes, rx))
    })
    .boxed();
    let stream = raw_response_stream(StreamResponse {
        status: StatusCode::OK,
        headers: HeaderMap::new(),
        bytes,
    });
    tokio::time::advance(std::time::Duration::from_secs(/*secs*/ 86400 * 365)).await;
    assert!(!tx.is_closed());
    drop(stream);
    tokio::task::yield_now().await;
    assert!(tx.is_closed());
}

#[tokio::test]
async fn missing_terminal_is_an_error() {
    let mut stream = raw_response_stream(StreamResponse {
        status: StatusCode::OK,
        headers: HeaderMap::new(),
        bytes: futures::stream::empty().boxed(),
    });
    assert!(stream.events.recv().await.unwrap().is_err());
    assert!(stream.events.recv().await.is_none());
}

#[tokio::test]
async fn bounded_queue_stops_reads_until_consumer_advances() {
    let (tx, rx) = mpsc::channel(/*buffer*/ 1);
    let bytes = futures::stream::unfold(rx, |mut rx| async {
        rx.recv().await.map(|bytes| (bytes, rx))
    })
    .boxed();
    let mut stream = raw_response_stream(StreamResponse {
        status: StatusCode::OK,
        headers: HeaderMap::new(),
        bytes,
    });
    let event = Bytes::from_static(b"data: {\"type\":\"future.event\"}\n\n");
    tx.send(Ok(event.clone())).await.unwrap();
    stream.events.recv().await.unwrap().unwrap();
    for _ in 0..8 {
        tokio::task::yield_now().await;
        let _ = tx.try_send(Ok(event.clone()));
    }
    assert!(matches!(
        tx.try_send(Ok(event)),
        Err(mpsc::error::TrySendError::Full(_))
    ));
    drop(stream);
    tx.closed().await;
}
