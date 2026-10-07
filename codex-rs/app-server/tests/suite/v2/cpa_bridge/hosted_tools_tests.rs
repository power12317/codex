use super::continuity_tests::exchange;
use super::*;
use core_test_support::load_default_config_for_test;
use pretty_assertions::assert_eq;
use test_case::test_case;

#[test_case(false; "responses")]
#[test_case(true; "responses_lite")]
#[tokio::test]
async fn cpa_hosted_tools_use_native_request_construction(lite: bool) -> Result<()> {
    let upstream = MockServer::start().await;
    let raw = "data: [DONE]\n\n";
    Mock::given(method("POST"))
        .and(path("/v1/responses"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(raw, "text/event-stream"))
        .expect(2)
        .mount(&upstream)
        .await;
    let home = TempDir::new()?;
    prepare_account(&upstream, &home)?;
    let config = load_default_config_for_test(&home).await;
    let mut model = codex_core::test_support::construct_model_info_offline("mock-model", &config);
    model.use_responses_lite = lite;
    let catalog = home.path().join("models.json");
    std::fs::write(&catalog, serde_json::to_vec(&json!({"models":[model]}))?)?;
    let worker_config = home.path().join("state/shared.json/config.toml");
    let original = std::fs::read_to_string(&worker_config)?;
    std::fs::write(
        &worker_config,
        format!(
            "model_catalog_json = {}\n{original}",
            serde_json::to_string(&catalog)?
        ),
    )?;
    let _runtime = launch(&upstream, &home).await?;
    let mut ws = connect(&home).await?;
    let image = json!({"type":"image_generation", "output_format":"png", "quality":"high", "future_option":{"keep":false}});
    let tools = json!([image, {"type":"tool_search"}]);
    let expected_tools = json!([image, {"type":"tool_search", "execution":"server", "description":"", "parameters":{"type":"object"}}]);
    let mut bodies = Vec::new();
    for choice in ["image_generation", "tool_search"] {
        let request = start(json!({"model":"mock-model", "tools":tools,
            "tool_choice":{"type":choice}, "instructions":"native instructions",
            "prompt_cache_key":"source-cache", "session_id":"source-session",
            "input":[{"type":"function_call", "id":"msg_old", "name":"fixture",
                "call_id":"call_unchanged", "arguments":"{}"}]}));
        let (logged, returned) = exchange(&mut ws, request).await?;
        assert_eq!(returned, raw.as_bytes());
        let body: Value = serde_json::from_str(logged["body"].as_str().expect("logged body"))?;
        let input = body["input"].as_array().expect("native input");
        if lite {
            assert!(body.get("tools").is_none());
            assert_eq!(input[0]["type"], json!("additional_tools"));
            assert_eq!(input[0]["tools"], expected_tools);
            assert_eq!(input[1]["role"], json!("developer"));
            assert_eq!(input[1]["content"][0]["text"], json!("native instructions"));
        } else {
            assert_eq!(body["tools"], expected_tools);
            assert_eq!(body["instructions"], json!("native instructions"));
        }
        let call = input.last().expect("historical call");
        assert_eq!(call["id"], json!("fc_old"));
        assert_eq!(call["call_id"], json!("call_unchanged"));
        assert_eq!(body["tool_choice"], json!({"type":choice}));
        assert_eq!(body["store"], json!(false));
        assert_eq!(
            body["prompt_cache_key"],
            body["client_metadata"]["session_id"]
        );
        assert_ne!(body["prompt_cache_key"], json!("source-cache"));
        bodies.push(body);
    }
    let received: Vec<_> = upstream
        .received_requests()
        .await
        .expect("recording enabled")
        .into_iter()
        .filter(|request| request.url.path() == "/v1/responses")
        .collect();
    assert_eq!(received.len(), 2);
    for (wire, body) in received.iter().zip(bodies) {
        assert_eq!(wire.body_json::<Value>()?, body);
        assert_eq!(wire.headers["authorization"], "Bearer fake-cpa-access");
        assert_eq!(wire.headers["originator"], "codex_cli_rs");
    }
    Ok(())
}
