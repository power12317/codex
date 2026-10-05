use super::*;
use codex_core::InferenceIdentity;
use pretty_assertions::assert_eq;
use serde_json::json;

#[test]
fn sse_mapping_handles_every_split_crlf_multiline_data_and_utf8() {
    let source = InferenceIdentity {
        session_id: Some("来源-session".into()),
        ..Default::default()
    };
    let mapped = source.map("scope").unwrap();
    let original = format!(
        ": keep\r\nid: untouched\r\nevent: response.metadata\r\ndata: {{\"type\":\"response.metadata\",\r\ndata: \"session_id\":\"{}\",\"text\":\"你好\"}}\r\n\r\ndata: [DONE]\n\n",
        mapped.session_id
    );
    let expected = format!(
        ": keep\r\nid: untouched\r\nevent: response.metadata\r\ndata: {}\r\n\r\ndata: [DONE]\n\n",
        json!({"type":"response.metadata", "session_id":"来源-session", "text":"你好"})
    );
    for split in 0..=original.len() {
        let mut stream = ResponseStream::new(
            IdentityResponse::new(source.clone(), mapped.clone()),
            "text/event-stream",
        );
        let mut output = stream.push(&original.as_bytes()[..split]).unwrap();
        output.extend(stream.push(&original.as_bytes()[split..]).unwrap());
        output.extend(stream.finish());
        assert_eq!(output, expected.as_bytes(), "split {split}");
    }
}

#[test]
fn unmodified_events_and_opaque_strings_retain_exact_bytes() {
    let source = InferenceIdentity {
        session_id: Some("source".into()),
        ..Default::default()
    };
    let mapped = source.map("scope").unwrap();
    let original = format!(
        ": keep\rdata: {{ \"type\": \"response.output_text.delta\", \"delta\": {:?} }}\r\rid: 42\ndata: not-json\n\n: tail without newline",
        mapped.session_id
    );
    let mut stream =
        ResponseStream::new(IdentityResponse::new(source, mapped), "text/event-stream");
    let mut output = Vec::new();
    for byte in original.as_bytes() {
        output.extend(stream.push(&[*byte]).unwrap());
    }
    output.extend(stream.finish());
    assert_eq!(output, original.as_bytes());
}

#[test]
fn json_body_and_unterminated_sse_metadata_are_restored_at_eof() {
    for (content_type, prefix) in [("application/json", ""), ("text/event-stream", "data: ")] {
        let source = InferenceIdentity {
            session_id: Some("source".into()),
            ..Default::default()
        };
        let mapped = source.map("scope").unwrap();
        let body = format!("{prefix}{}", json!({"session_id":mapped.session_id}));
        let mut stream = ResponseStream::new(IdentityResponse::new(source, mapped), content_type);
        assert!(stream.push(body.as_bytes()).unwrap().is_empty());
        assert_eq!(
            stream.finish(),
            format!("{prefix}{}", json!({"session_id":"source"})).as_bytes()
        );
    }
}

#[test]
fn mapping_buffers_have_a_hard_limit() {
    let source = InferenceIdentity::default();
    let mapped = source.map("scope").unwrap();
    let mut stream =
        ResponseStream::new(IdentityResponse::new(source, mapped), "text/event-stream");
    assert!(stream.push(&vec![b'x'; MAX_EVENT_BYTES + 1]).is_err());
}

#[test]
fn heartbeat_lines_are_forwarded_without_waiting_for_a_blank_line() {
    let source = InferenceIdentity::default();
    let mapped = source.map("scope").unwrap();
    let mut stream =
        ResponseStream::new(IdentityResponse::new(source, mapped), "text/event-stream");
    assert_eq!(stream.push(b": ping\n").unwrap(), b": ping\n");
    assert_eq!(
        stream.push(b": another ping\r\n").unwrap(),
        b": another ping\r\n"
    );
    assert!(stream.finish().is_empty());
}
