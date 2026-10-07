use super::*;
use pretty_assertions::assert_eq;

#[test]
fn normalizes_function_envelopes_without_losing_schema_or_choice() {
    let parameters = json!({"type":"object", "properties":{"value":{"type":"string", "encrypted":true}}, "required":["value"], "additionalProperties":false});
    let request = json!({"tools":[{"type":"function", "function":{"name":"run", "description":"test", "parameters":parameters, "strict":true, "defer_loading":false}}], "tool_choice":{"type":"function", "function":{"name":"run"}}});
    let parsed = InferenceTools::parse(request.as_object().unwrap()).unwrap();
    assert_eq!(
        serde_json::to_value(parsed.native).unwrap(),
        json!([{"type":"function", "name":"run", "description":"test", "parameters":parameters, "strict":true, "defer_loading":false}])
    );
    assert_eq!(
        parsed.choice,
        Some(json!({"type":"function", "name":"run"}))
    );
}

#[test]
fn reports_unsupported_tool_and_schema_at_their_location() {
    for tool in [
        json!({"type":"computer"}),
        json!({"type":"function", "name":"run", "parameters":{"type":"object", "unknown_constraint":true}}),
    ] {
        let request = json!({"tools":[tool]});
        let error = InferenceTools::parse(request.as_object().unwrap())
            .err()
            .unwrap();
        assert!(format!("{error:#}").starts_with("tools[0]:"));
    }
}

#[test]
fn native_tools_preserve_namespaces_search_options_and_custom_text_format() {
    let request = json!({"tools":[
        {"type":"namespace", "name":"external", "description":"client tools", "tools":[{"type":"custom", "name":"exec", "format":{"type":"text"}}]},
        {"type":"web_search", "external_web_access":false, "search_content_types":["text", "image"]}
    ]});
    let parsed = InferenceTools::parse(request.as_object().unwrap()).unwrap();
    assert_eq!(
        serde_json::to_value(parsed.native).unwrap(),
        json!([
            {"type":"namespace", "name":"external", "description":"client tools", "tools":[{"type":"custom", "name":"exec", "description":"", "format":{"type":"text"}}]},
            {"type":"web_search", "external_web_access":false, "search_content_types":["text", "image"]}
        ])
    );
}

#[test]
fn built_in_search_choice_needs_no_function_name() {
    let request = json!({"tools":[{"type":"web_search", "external_web_access":false}], "tool_choice":{"type":"web_search"}});
    let parsed = InferenceTools::parse(request.as_object().unwrap()).unwrap();
    assert_eq!(parsed.choice, Some(json!({"type":"web_search"})));
}

#[test]
fn hosted_image_generation_and_tool_search_use_native_serialization() {
    let request = json!({"tools":[
        {"type":"image_generation","output_format":"png","future_option":{"explicit":false}},
        {"type":"tool_search","execution":"client","description":"Search tools","parameters":{"type":"object"}}
    ],"tool_choice":{"type":"image_generation"}});
    let parsed =
        InferenceTools::parse(request.as_object().expect("request")).expect("native tools");
    assert_eq!(
        serde_json::to_value(&parsed.native).expect("serialize"),
        request["tools"]
    );
    assert_eq!(
        codex_tools::create_tools_json_for_responses_lite(&parsed.native).expect("lite tools"),
        request["tools"].as_array().expect("tools").clone()
    );
    assert_eq!(parsed.choice, Some(json!({"type":"image_generation"})));
}
