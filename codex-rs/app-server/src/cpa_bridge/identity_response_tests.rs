use super::*;
use pretty_assertions::assert_eq;
use serde_json::json;

#[test]
fn restores_only_identity_paths_and_preserves_opaque_payloads() {
    let source = InferenceIdentity {
        session_id: Some("source-session".into()),
        thread_id: Some("source-child".into()),
        parent_thread_id: Some("source-session".into()),
        prompt_cache_key: Some("source-cache".into()),
        ..Default::default()
    };
    let mut mapped = source.map("scope").unwrap();
    mapped.turn_id = Some("synthetic-turn".into());
    let opaque = json!({"id":mapped.session_id, "call_id":mapped.thread_id, "arguments":json!({"session_id":mapped.session_id}).to_string(), "text":mapped.thread_id});
    let native_metadata = json!({"session_id":mapped.session_id, "thread_id":mapped.thread_id, "parent_thread_id":mapped.parent_thread_id, "turn_id":"synthetic-turn", "window_id":mapped.window_id});
    let mut value = json!({"type":"response.completed", "response":{"id":mapped.session_id,
        "session_id":mapped.session_id, "thread_id":mapped.thread_id, "prompt_cache_key":mapped.session_id,
        "client_metadata":{"x-codex-turn-metadata":native_metadata.to_string()},
        "output":[opaque], "metadata":{"session_id":mapped.session_id}},
        "headers":{"Thread-Id":[mapped.thread_id], "x-codex-turn-state":[mapped.session_id]}});
    assert!(IdentityResponse::new(source, mapped.clone()).restore(&mut value));
    assert_eq!(
        value,
        json!({"type":"response.completed", "response":{"id":mapped.session_id,
        "session_id":"source-session", "thread_id":"source-child", "prompt_cache_key":"source-cache",
        "client_metadata":{"x-codex-turn-metadata":json!({"session_id":"source-session", "thread_id":"source-child", "parent_thread_id":"source-session"}).to_string()},
        "output":[opaque], "metadata":{"session_id":mapped.session_id}},
        "headers":{"Thread-Id":["source-child"], "x-codex-turn-state":[mapped.session_id]}})
    );
}

#[test]
fn missing_source_ids_are_removed_only_when_equal_to_generated_ids() {
    let source = InferenceIdentity::default();
    let mut mapped = source.map("scope").unwrap();
    mapped.turn_id = Some("generated".into());
    let mut value = json!({"sessionId":mapped.session_id, "thread_id":"a-server-owned-id", "turn_id":"generated", "response":{"turn_id":"server-turn"}});
    assert!(IdentityResponse::new(source, mapped).restore(&mut value));
    assert_eq!(
        value,
        json!({"thread_id":"a-server-owned-id", "response":{"turn_id":"server-turn"}})
    );
}

#[test]
fn failure_restores_structured_body_and_headers_but_not_diagnostic_ids() {
    let source = InferenceIdentity {
        session_id: Some("source".into()),
        ..Default::default()
    };
    let mapped = source.map("scope").unwrap();
    let mut error = codex_app_server_protocol::JSONRPCErrorError {
        code: -32000,
        message: "error".into(),
        data: Some(json!({
            "httpStatus":400, "body":json!({"error":{"session_id":mapped.session_id, "message":mapped.session_id}}).to_string(),
            "headers":{"session-id":[mapped.session_id], "x-request-id":[mapped.session_id], "content-length":["100"], "etag":["etag"]}
        })),
    };
    IdentityResponse::new(source, mapped.clone()).failure(&mut error);
    assert_eq!(
        error.data,
        Some(json!({"httpStatus":400,
            "body":json!({"error":{"session_id":"source", "message":mapped.session_id}}).to_string(),
            "headers":{"session-id":["source"], "x-request-id":[mapped.session_id]}
        }))
    );
}
