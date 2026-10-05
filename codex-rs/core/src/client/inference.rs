//! Prepare one caller-owned Responses request without creating an agent turn.
use super::*;
use codex_protocol::ResponseItemId;
use codex_protocol::models::BaseInstructions;
use serde_json::Map;
use serde_json::Value;
use serde_json::json;

const WORKER_OWNED_METADATA_KEYS: &[&str] = &[
    "x-codex-installation-id",
    "x-codex-turn-metadata",
    "x-codex-window-id",
    "x-codex-parent-thread-id",
    "x-codex-routing-hint",
    "x-codex-turn-state",
    "x-openai-subagent",
    "installation_id",
    "session_id",
    "thread_id",
    "turn_id",
    "window_id",
    "context_window_id",
    "parent_thread_id",
    "parent_turn_id",
    "root_turn_id",
    "agent_name",
    "request_kind",
    "thread_source",
    "turn_trigger",
    "sandbox",
    "sandbox_mode",
    "auto_review_enabled",
    "node_repl_auto_review_required",
    "node_repl_disabled",
    "workspaces",
    "tool_namespaces_info",
    "turn_started_at_unix_ms",
    "history_ingest_requested",
    "analytics_enabled",
    "mcp_attribution",
];

/// Uses the native prompt/model builder and request options, without response parsing
/// or a tool runtime. Caller-only Responses options survive serialization explicitly.
pub async fn prepare_inference_request(
    session: &InferenceSession,
    model_info: &ModelInfo,
    request: &Map<String, Value>,
) -> anyhow::Result<(Value, ApiResponsesOptions)> {
    let client = &session.session.client;
    let metadata = &session.metadata;
    let mut input = request.get("input").cloned().unwrap_or_else(|| json!([]));
    if input.is_string() {
        input = json!([{"type":"message","role":"user","content":[{"type":"input_text","text":input}]}]);
    }
    let input_items = input
        .as_array_mut()
        .ok_or_else(|| anyhow::anyhow!("input must be text or an array"))?;
    for item in &mut *input_items {
        if item.get("role").is_some() && item.get("type").is_none() {
            item["type"] = json!("message");
        }
        if item["type"] == "message" && item["content"].is_string() {
            item["content"] = json!([{"type":"input_text","text":item["content"]}]);
        }
    }
    let time = super::inference_context::TimeContext::now();
    if !time.apply(input_items) {
        input_items.push(time.message());
    }
    let mut input: Vec<ResponseItem> = serde_json::from_value(input)?;
    for item in &mut input {
        if let Some(prefix) = item.id_prefix()
            && let Some(id) = item.id()
            && let Some((actual, suffix)) = id.split_once('_')
            && !suffix.is_empty()
            && actual != prefix
        {
            item.set_id(Some(ResponseItemId::with_suffix(prefix, suffix)));
        }
    }
    anyhow::ensure!(
        !input.iter().any(|item| matches!(item, ResponseItem::Other)),
        "unsupported input item"
    );
    let tools = super::inference_tools::InferenceTools::parse(request)?;
    let prompt = Prompt {
        input,
        tools: tools.native.into(),
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
        metadata,
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
                | "tool_choice"
                | "reasoning"
                | "parallel_tool_calls"
                | "store"
                | "stream"
                | "service_tier"
                | "client_metadata"
                | "headers"
                | "authorization"
                | "Authorization"
                | "cookies"
                | "installation_id"
                | "account_id"
                | "x-codex-installation-id"
                | "x-codex-turn-metadata"
                | "x-codex-window-id"
                | "x-codex-parent-thread-id"
                | "x-codex-routing-hint"
                | "x-codex-turn-state"
                | "session_id"
                | "thread_id"
                | "turn_id"
                | "window_id"
                | "context_window_id"
                | "parent_thread_id"
                | "parent_turn_id"
                | "root_turn_id"
                | "prompt_cache_key"
                | "timezone"
                | "current_date"
        ) {
            body[key] = value.clone();
        }
    }
    if let Some(choice) = tools.choice {
        body["tool_choice"] = choice;
    }
    merge_caller_client_metadata(&mut body, request);
    time.metadata(&mut body, request);
    if let Some(reasoning) = reasoning.and_then(Value::as_object) {
        for (key, value) in reasoning {
            if !matches!(key.as_str(), "effort" | "summary") {
                body["reasoning"][key] = value.clone();
            }
        }
    }
    let options = session
        .session
        .build_responses_options(metadata, Compression::None, model_info.use_responses_lite)
        .await;
    Ok((body, options))
}

fn merge_caller_client_metadata(body: &mut Value, request: &Map<String, Value>) {
    let Some(caller_metadata) = request.get("client_metadata").and_then(Value::as_object) else {
        return;
    };
    let Some(native_metadata) = body
        .get_mut("client_metadata")
        .and_then(Value::as_object_mut)
    else {
        return;
    };

    for (key, value) in caller_metadata {
        if key == "prompt_cache_key"
            || WORKER_OWNED_METADATA_KEYS.contains(&key.as_str())
            || native_metadata.contains_key(key)
        {
            continue;
        }
        native_metadata.insert(key.clone(), value.clone());
    }
}
