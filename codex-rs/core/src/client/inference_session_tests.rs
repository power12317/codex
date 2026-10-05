use super::InferenceIdentity;
use pretty_assertions::assert_eq;
use serde_json::json;

#[test]
fn session_identity_groups_threads_and_ignores_cache_affinity() {
    for (request, expected) in [
        (
            json!({"thread_id":"thread", "session_id":"session", "prompt_cache_key":"cache"}),
            Some("session"),
        ),
        (
            json!({"session_id":"session", "prompt_cache_key":"cache"}),
            Some("session"),
        ),
        (json!({"prompt_cache_key":"cache"}), None),
    ] {
        let identity = InferenceIdentity::from_request(request.as_object().unwrap());
        assert_eq!(identity.session_key(), expected);
    }
}

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
            ..Default::default()
        }
    );
}

#[test]
fn hierarchy_maps_root_children_and_parent_links_within_the_same_scope() {
    let root = InferenceIdentity {
        session_id: Some("s".into()),
        thread_id: Some("s".into()),
        ..Default::default()
    };
    let root_map = root.map("credential/caller").unwrap();
    assert_eq!(root_map.session_id, root_map.thread_id);
    let child = InferenceIdentity {
        thread_id: Some("child".into()),
        parent_thread_id: Some("s".into()),
        ..root.clone()
    };
    let child_map = child.map("credential/caller").unwrap();
    assert_eq!(child_map.session_id, root_map.session_id);
    assert_ne!(child_map.thread_id, root_map.thread_id);
    assert_eq!(child_map.parent_thread_id, Some(root_map.thread_id));
    let grandchild = InferenceIdentity {
        thread_id: Some("grandchild".into()),
        parent_thread_id: Some("child".into()),
        ..root.clone()
    };
    let mapped = grandchild.map("credential/caller").unwrap();
    assert_eq!(mapped.parent_thread_id, Some(child_map.thread_id));
    assert_eq!(mapped.session_id, root_map.session_id);
    assert_ne!(
        root.map("other-credential/caller").unwrap().session_id,
        root_map.session_id
    );
    assert_ne!(
        InferenceIdentity {
            session_id: Some("other-session".into()),
            ..child
        }
        .map("credential/caller")
        .unwrap()
        .session_id,
        root_map.session_id
    );
}

#[test]
fn title_classification_requires_explicit_metadata() {
    let prompt_only =
        serde_json::json!({"input":"generate a thread_title", "instructions":"title_generation"});
    assert!(!InferenceIdentity::from_request(prompt_only.as_object().unwrap()).is_title());
    for request in [
        serde_json::json!({"thread_source":"thread_title"}),
        serde_json::json!({"client_metadata":{"thread_source":"thread_title"}}),
        serde_json::json!({"client_metadata":{"x-codex-turn-metadata":"{\"thread_source\":\"thread_title\"}"}}),
        serde_json::json!({"request_kind":"title_generation"}),
    ] {
        assert!(InferenceIdentity::from_request(request.as_object().unwrap()).is_title());
    }
}
