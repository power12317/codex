use super::InferenceIdentity;
use pretty_assertions::assert_eq;
use serde_json::json;

#[test]
fn source_identity_merges_top_level_flat_and_nested_metadata() {
    let request = json!({"session_id":"top-session", "turn_id":null, "prompt_cache_key":"",
        "client_metadata":{"session_id":"flat-session", "thread_id":"flat-thread",
            "x-codex-turn-metadata":json!({"thread_id":"nested-thread", "turn_id":"nested-turn",
                "prompt_cache_key":"nested-cache", "parent_turn_id":"parent", "root_turn_id":"root"}).to_string()}});
    assert_eq!(
        InferenceIdentity::from_request(request.as_object().unwrap()),
        InferenceIdentity {
            session_id: Some("top-session".into()),
            thread_id: Some("flat-thread".into()),
            turn_id: Some("nested-turn".into()),
            prompt_cache_key: Some("nested-cache".into()),
            parent_turn_id: Some("parent".into()),
            root_turn_id: Some("root".into()),
        }
    );
}
