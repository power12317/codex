//! Prepare one caller-owned Responses request without creating an agent turn.
use super::*;
use crate::config::Config;
use codex_protocol::models::BaseInstructions;
use codex_tools::FreeformTool;
use codex_tools::ResponsesApiTool;
use codex_tools::ToolSpec;
use serde_json::Map;
use serde_json::Value;
use serde_json::json;

/// Uses the native prompt/model builder and request options, without response parsing
/// or a tool runtime. Caller-only Responses options survive serialization explicitly.
pub async fn prepare_inference_request(
    config: &Config,
    auth: Arc<AuthManager>,
    model_info: &ModelInfo,
    installation_id: &str,
    session_id: &str,
    request: &Map<String, Value>,
) -> anyhow::Result<(Value, ApiResponsesOptions)> {
    let thread_id = Uuid::new_v5(&Uuid::NAMESPACE_OID, session_id.as_bytes()).to_string();
    let client = ModelClient::new(
        Some(auth),
        AgentIdentityAuthPolicy::JwtOnly,
        ThreadId::from_string(&thread_id)?,
        config.model_provider.clone(),
        SessionSource::Cli,
        "codex_cli_rs".into(),
        request
            .get("text")
            .and_then(|text| text.get("verbosity"))
            .map(|value| serde_json::from_value(value.clone()))
            .transpose()?
            .or(config.model_verbosity),
        /*content_item_kinds_enabled*/ false,
        /*reasoning_effort_override_enabled*/ false,
        /*enable_request_compression*/ false,
        /*include_timing_metrics*/ false,
        /*beta_features_header*/ None,
        /*concurrent_reasoning_summaries_enabled*/ false,
        /*attestation_provider*/ None,
        config.http_client_factory(),
        config.workspace_routing_context(),
        Vec::new(),
    );
    let metadata = CodexResponsesMetadata::new(
        installation_id.into(),
        session_id.into(),
        thread_id,
        session_id.into(),
    );
    let mut input = request.get("input").cloned().unwrap_or_else(|| json!([]));
    if input.is_string() {
        input = json!([{"type":"message","role":"user","content":[{"type":"input_text","text":input}]}]);
    }
    for item in input
        .as_array_mut()
        .ok_or_else(|| anyhow::anyhow!("input must be text or an array"))?
    {
        if item.get("role").is_some() && item.get("type").is_none() {
            item["type"] = json!("message");
        }
        if item["type"] == "message" && item["content"].is_string() {
            item["content"] = json!([{"type":"input_text","text":item["content"]}]);
        }
    }
    let input: Vec<ResponseItem> = serde_json::from_value(input)?;
    anyhow::ensure!(
        !input.iter().any(|item| matches!(item, ResponseItem::Other)),
        "unsupported input item"
    );
    let mut tools = Vec::new();
    if let Some(caller_tools) = request.get("tools") {
        for tool in caller_tools
            .as_array()
            .ok_or_else(|| anyhow::anyhow!("tools must be an array"))?
        {
            tools.push(match tool["type"].as_str() {
                Some("function") => {
                    let parameters = tool
                        .get("parameters")
                        .cloned()
                        .unwrap_or_else(|| json!({"type":"object"}));
                    let schema = serde_json::from_value(parameters.clone())?;
                    anyhow::ensure!(
                        serde_json::to_value(&schema)? == parameters,
                        "unsupported function parameter schema"
                    );
                    ToolSpec::Function(ResponsesApiTool {
                        name: tool["name"]
                            .as_str()
                            .ok_or_else(|| anyhow::anyhow!("function name required"))?
                            .into(),
                        description: tool["description"].as_str().unwrap_or_default().into(),
                        strict: tool
                            .get("strict")
                            .map(|value| serde_json::from_value(value.clone()))
                            .transpose()?
                            .unwrap_or(false),
                        defer_loading: tool
                            .get("defer_loading")
                            .map(|value| serde_json::from_value(value.clone()))
                            .transpose()?,
                        parameters: schema,
                        output_schema: None,
                    })
                }
                Some("custom") => {
                    ToolSpec::Freeform(serde_json::from_value::<FreeformTool>(tool.clone())?)
                }
                _ => anyhow::bail!("only caller-executed function/custom tools are supported"),
            });
        }
    }
    let prompt = Prompt {
        input,
        tools: tools.into(),
        parallel_tool_calls: request
            .get("parallel_tool_calls")
            .map(|v| serde_json::from_value(v.clone()))
            .transpose()?
            .unwrap_or(true),
        base_instructions: BaseInstructions {
            text: request
                .get("instructions")
                .map(|v| serde_json::from_value(v.clone()))
                .transpose()?
                .unwrap_or_default(),
            ..Default::default()
        },
        output_schema: request
            .get("text")
            .and_then(|t| t.get("format"))
            .and_then(|f| f.get("schema"))
            .cloned(),
        output_schema_strict: request
            .get("text")
            .and_then(|t| t.get("format"))
            .and_then(|f| f.get("strict"))
            .map(|v| serde_json::from_value(v.clone()))
            .transpose()?
            .unwrap_or(true),
        cyber_access_program: None,
    };
    let reasoning = request.get("reasoning");
    let effort = reasoning
        .and_then(|r| r.get("effort"))
        .map(|v| serde_json::from_value(v.clone()))
        .transpose()?;
    let summary = reasoning
        .and_then(|r| r.get("summary"))
        .map(|v| serde_json::from_value(v.clone()))
        .transpose()?
        .unwrap_or(ReasoningSummaryConfig::None);
    let service_tier = request
        .get("service_tier")
        .map(|v| serde_json::from_value(v.clone()))
        .transpose()?;
    let built = client.build_responses_request(
        &prompt,
        model_info,
        effort,
        summary,
        service_tier,
        &metadata,
        /*include_internal*/ false,
    )?;
    let mut body = serde_json::to_value(built)?;
    // The native DTO's tool_choice is a string. Keep caller forced-tool objects,
    // extensions and additional Responses options without bypassing prompt construction.
    for (key, value) in request {
        if !matches!(
            key.as_str(),
            "model"
                | "input"
                | "instructions"
                | "tools"
                | "reasoning"
                | "parallel_tool_calls"
                | "store"
                | "stream"
                | "service_tier"
                | "client_metadata"
        ) {
            body[key] = value.clone();
        }
    }
    if let Some(reasoning) = reasoning.and_then(Value::as_object) {
        for (key, value) in reasoning {
            if !matches!(key.as_str(), "effort" | "summary") {
                body["reasoning"][key] = value.clone();
            }
        }
    }
    let options = client
        .new_session()
        .build_responses_options(&metadata, Compression::None, model_info.use_responses_lite)
        .await;
    Ok((body, options))
}
