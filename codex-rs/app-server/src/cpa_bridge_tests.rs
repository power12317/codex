use super::*;
use pretty_assertions::assert_eq;

#[test]
fn request_semantics_are_preserved_and_unsupported_state_is_rejected() {
    let request = json!({"model":"fixture", "stream":true, "instructions":"exact instructions", "tools":[{"type":"function","name":"shell","parameters":{"type":"object"}}], "tool_choice":{"type":"function","name":"shell"}, "client_metadata":{"x-codex-installation-id":"caller-installation","business_tag":"keep"}, "headers":{"authorization":"caller-secret"}, "extension":{"future":true}});
    let params: CpaInferenceStartParams = serde_json::from_value(json!({"requestId":"r", "credentialId":"worker", "operation":"responses", "sourceFormat":"openai-response", "sessionId":"scope", "request":request})).unwrap();
    validate(&params).unwrap();
    assert_eq!(Value::Object(params.request.clone()), request);
    for (key, value) in [
        ("previous_response_id", json!(null)),
        ("generate", json!(false)),
        ("background", json!(true)),
        ("store", json!(true)),
        ("stream", json!(false)),
    ] {
        let mut changed = params.clone();
        changed.request.insert(key.into(), value);
        assert_eq!(
            validate(&changed).unwrap_err().data,
            Some(json!({"httpStatus":400}))
        );
    }
}
