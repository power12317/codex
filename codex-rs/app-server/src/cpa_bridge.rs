//! CPA IPC v1 boundary. This module deliberately has no ThreadManager or tool runtime.
use crate::outgoing_message::ConnectionId;
use crate::outgoing_message::ConnectionRequestId;
use crate::outgoing_message::OutgoingMessageSender;
use codex_api::ApiError;
use codex_api::RawResponseStream;
use codex_api::ReqwestTransport;
use codex_api::ResponsesClient;
use codex_api::TransportError;
use codex_app_server_protocol::*;
use codex_core::config::Config;
use codex_http_client::ClientRouteClass;
use codex_http_client::HttpClientBuilder;
use codex_login::AuthManager;
use codex_login::CodexAuth;
use codex_model_provider::SharedModelProvider;
use codex_model_provider::auth_provider_from_auth;
use codex_model_provider::create_model_provider;
use serde_json::Value;
use serde_json::json;
use std::collections::BTreeMap;
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;
use std::time::Duration;
use tokio_util::sync::CancellationToken;

const UPSTREAM: &str = "985cf47a4eb6084b2ff6b30ebdb1216acda85bb4";

type BridgeResult<T> = std::result::Result<T, JSONRPCErrorError>;

pub(crate) struct CpaBridge {
    credential_id: Option<String>,
    auth: Arc<AuthManager>,
    config: Arc<Config>,
    provider: Option<SharedModelProvider>,
    installation_id: String,
    outgoing: Arc<OutgoingMessageSender>,
    active: Mutex<HashMap<ConnectionId, (String, CancellationToken)>>,
}

impl CpaBridge {
    pub(crate) fn new(
        config: Arc<Config>,
        auth: Arc<AuthManager>,
        installation_id: String,
        outgoing: Arc<OutgoingMessageSender>,
    ) -> Arc<Self> {
        let effective = config.config_layer_stack.effective_config();
        let bridge = effective.get("cpa_bridge");
        let credential_id = bridge
            .filter(|v| v.get("enabled").and_then(toml::Value::as_bool) == Some(true))
            .and_then(|v| v.get("credential_id"))
            .and_then(toml::Value::as_str)
            .filter(|s| !s.is_empty() && s.len() <= 256)
            .map(str::to_owned);
        let provider = credential_id
            .as_ref()
            .map(|_| create_model_provider(config.model_provider.clone(), Some(auth.clone())));
        Arc::new(Self {
            credential_id,
            auth,
            config,
            provider,
            installation_id,
            outgoing,
            active: Mutex::new(HashMap::new()),
        })
    }

    pub(crate) fn enabled(&self) -> bool {
        self.credential_id.is_some()
    }

    pub(crate) fn capabilities(&self) -> BridgeResult<CpaCapabilitiesReadResponse> {
        let credential_id = self
            .credential_id
            .clone()
            .ok_or_else(|| failure(/*status*/ 404, "CPA bridge is disabled"))?;
        let auth = self
            .auth
            .auth_cached()
            .filter(|auth| matches!(auth, CodexAuth::Chatgpt(_)));
        Ok(CpaCapabilitiesReadResponse {
            protocol_version: 1,
            runtime_version: env!("CARGO_PKG_VERSION").into(),
            upstream_revision: UPSTREAM.into(),
            credential_id,
            account_id: auth.as_ref().and_then(CodexAuth::get_account_id),
            auth_mode: auth.map(|_| "chatgpt".into()),
            execution_mode: "inference-only".into(),
            raw_events: true,
            operations: vec!["responses".into()],
            persistent_sessions: false,
        })
    }

    pub(crate) fn disconnect(&self, connection_id: ConnectionId) {
        if let Some((_, token)) = self
            .active
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .remove(&connection_id)
        {
            token.cancel();
        }
    }

    pub(crate) fn cancel(
        &self,
        connection_id: ConnectionId,
        params: CpaInferenceCancelParams,
    ) -> BridgeResult<CpaInferenceCancelResponse> {
        let active = self
            .active
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some((request_id, token)) = active.get(&connection_id) {
            if request_id != &params.request_id {
                return Err(failure(
                    /*status*/ 404,
                    "Unknown requestId for this connection",
                ));
            }
            token.cancel();
        }
        Ok(CpaInferenceCancelResponse {})
    }

    pub(crate) fn start(
        self: &Arc<Self>,
        id: ConnectionRequestId,
        params: CpaInferenceStartParams,
    ) -> BridgeResult<()> {
        validate(&params)?;
        if self.credential_id.as_deref() != Some(params.credential_id.as_str()) {
            return Err(failure(/*status*/ 403, "credentialId mismatch"));
        }
        self.bound_auth(&params.account_id)?;
        let token = CancellationToken::new();
        {
            let mut active = self
                .active
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if active.contains_key(&id.connection_id) {
                return Err(failure(
                    /*status*/ 409,
                    "One inference per connection is allowed",
                ));
            }
            active.insert(id.connection_id, (params.request_id.clone(), token.clone()));
        }
        let bridge = self.clone();
        tokio::spawn(async move {
            let started = AtomicBool::new(/*v*/ false);
            let mut auth_changes = bridge.auth.auth_change_receiver();
            let result = tokio::select! {
                biased;
                _ = token.cancelled() => Err(failure(/*status*/ 499, "Inference cancelled")),
                _ = async {
                    loop {
                        if auth_changes.changed().await.is_err() || bridge.bound_auth(&params.account_id).is_err() { break; }
                    }
                } => Err(failure(/*status*/ 401, "Managed account changed")),
                result = bridge.run(&id, &params, &started) => result,
            };
            if let Err(error) = result {
                if started.load(Ordering::Acquire) {
                    bridge
                        .outgoing
                        .send_server_notification_to_connections(
                            &[id.connection_id],
                            ServerNotification::CpaInferenceError(CpaInferenceErrorNotification {
                                request_id: params.request_id.clone(),
                                http_status: error
                                    .data
                                    .as_ref()
                                    .and_then(|d| d["httpStatus"].as_u64())
                                    .unwrap_or(502)
                                    as u16,
                                message: error.message,
                            }),
                        )
                        .await;
                } else {
                    bridge.outgoing.send_error(id.clone(), error).await;
                }
            }
            if started.load(Ordering::Acquire) {
                bridge
                    .outgoing
                    .send_server_notification_to_connections(
                        &[id.connection_id],
                        ServerNotification::CpaInferenceCompleted(
                            CpaInferenceCompletedNotification {
                                request_id: params.request_id,
                            },
                        ),
                    )
                    .await;
            }
            bridge
                .active
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .remove(&id.connection_id);
        });
        Ok(())
    }

    fn bound_auth(&self, account_id: &str) -> BridgeResult<CodexAuth> {
        let auth = self
            .auth
            .auth_cached()
            .filter(|a| matches!(a, CodexAuth::Chatgpt(_)))
            .ok_or_else(|| failure(/*status*/ 401, "Managed ChatGPT login required"))?;
        if auth.get_account_id().as_deref() != Some(account_id) {
            return Err(failure(/*status*/ 403, "accountId mismatch"));
        }
        Ok(auth)
    }

    async fn open(&self, params: &CpaInferenceStartParams) -> BridgeResult<RawResponseStream> {
        // Only AuthManager acquires/refreshes credentials. No tokens cross IPC.
        let (auth, factory) = tokio::time::timeout(
            Duration::from_secs(/*secs*/ 30),
            self.auth.auth_with_http_client_factory(),
        )
        .await
        .map_err(|_| failure(/*status*/ 504, "Managed authentication timed out"))?
        .ok_or_else(|| failure(/*status*/ 401, "Managed ChatGPT login required"))?;
        self.bound_auth(&params.account_id)?;
        if !matches!(auth, CodexAuth::Chatgpt(_))
            || auth.get_account_id().as_deref() != Some(&params.account_id)
        {
            return Err(failure(/*status*/ 401, "Managed account changed"));
        }
        // Reject custom credential providers; the bridge has exactly one auth owner.
        let model_provider = self
            .provider
            .as_ref()
            .ok_or_else(|| failure(/*status*/ 404, "CPA bridge is disabled"))?;
        let info = model_provider.info();
        if !info.is_openai()
            || !info.requires_openai_auth
            || info.auth.is_some()
            || info.env_key.is_some()
            || info.experimental_bearer_token.is_some()
            || info.aws.is_some()
        {
            return Err(failure(
                /*status*/ 400,
                "CPA requires the managed OpenAI provider",
            ));
        }
        let revision = *self.auth.auth_change_receiver().borrow();
        let resolved = model_provider
            .responses_api_provider(&self.config.workspace_routing_context())
            .await
            .map_err(|_| failure(/*status*/ 502, "Unable to resolve managed provider"))?;
        if *self.auth.auth_change_receiver().borrow() != revision {
            return Err(failure(
                /*status*/ 409,
                "Authentication changed during request setup; retry",
            ));
        }
        self.bound_auth(&params.account_id)?;
        let mut provider = resolved.provider;
        // A failed stream must never cause another inference attempt.
        // RetryPolicy counts retries after the initial attempt. Zero means one POST.
        provider.retry.max_attempts = 0;
        let client = HttpClientBuilder::new()
            .default_headers(codex_login::default_client::default_headers())
            .with_chatgpt_cloudflare_cookie_store()
            .without_request_logging()
            .without_redirects()
            .build_respecting_outbound_proxy_policy(
                &factory,
                &provider.url_for_path("responses"),
                ClientRouteClass::Api,
            )
            .map_err(|_| {
                failure(
                    /*status*/ 502,
                    "Unable to construct managed HTTP client",
                )
            })?;
        let mut headers = codex_api::build_session_headers(
            Some(params.session_id.clone()),
            /*thread_id*/ None,
        );
        headers.insert(
            "x-codex-installation-id",
            self.installation_id
                .parse()
                .map_err(|_| failure(/*status*/ 500, "Invalid installation identity"))?,
        );
        ResponsesClient::new(
            ReqwestTransport::from_http_client(client),
            provider,
            auth_provider_from_auth(&auth),
        )
        .stream_raw(Value::Object(params.request.clone()), headers)
        .await
        .map_err(api_failure)
    }

    async fn run(
        &self,
        id: &ConnectionRequestId,
        params: &CpaInferenceStartParams,
        started: &AtomicBool,
    ) -> BridgeResult<()> {
        let mut recovery = self.auth.unauthorized_recovery();
        let mut stream = loop {
            match self.open(params).await {
                Ok(stream) => break stream,
                Err(error)
                    if error.data.as_ref().is_some_and(|d| d["httpStatus"] == 401)
                        && recovery.has_next() =>
                {
                    tokio::time::timeout(Duration::from_secs(/*secs*/ 30), recovery.next())
                        .await
                        .map_err(|_| {
                            failure(/*status*/ 504, "Managed authentication timed out")
                        })?
                        .map_err(|_| {
                            failure(
                                /*status*/ 401,
                                "Managed authentication recovery failed",
                            )
                        })?;
                }
                Err(error) => return Err(error),
            }
        };
        // Allowlist excludes cookies, bearer credentials, and internal routing headers.
        let mut headers = BTreeMap::new();
        for name in [
            "content-type",
            "x-request-id",
            "retry-after",
            "openai-model",
            "openai-processing-ms",
        ] {
            let values: Vec<_> = stream
                .headers
                .get_all(name)
                .iter()
                .filter_map(|v| v.to_str().ok().map(str::to_owned))
                .collect();
            if !values.is_empty() {
                headers.insert(name.to_owned(), values);
            }
        }
        self.outgoing
            .send_response(
                id.clone(),
                CpaInferenceStartResponse {
                    request_id: params.request_id.clone(),
                    status_code: stream.status.as_u16(),
                    headers,
                },
            )
            .await;
        started.store(true, Ordering::Release);
        let mut terminal = false;
        while let Some(event) = stream.events.recv().await {
            let event = event.map_err(api_failure)?;
            terminal = matches!(
                event["type"].as_str(),
                Some("response.completed" | "response.incomplete" | "response.failed" | "error")
            );
            // Protocol metadata may contain upstream headers. Fail closed instead of
            // leaking secrets or silently rewriting a supposedly lossless event.
            for headers in [
                event.get("headers"),
                event.get("response").and_then(|r| r.get("headers")),
            ]
            .into_iter()
            .flatten()
            {
                if headers.as_object().is_some_and(|headers| {
                    headers.keys().any(|k| {
                        matches!(
                            k.to_ascii_lowercase().as_str(),
                            "authorization" | "proxy-authorization" | "cookie" | "set-cookie"
                        )
                    })
                }) {
                    return Err(failure(
                        /*status*/ 502,
                        "Upstream event contains private transport metadata",
                    ));
                }
            }
            if !self
                .outgoing
                .send_server_notification_to_connection_and_wait(
                    id.connection_id,
                    ServerNotification::CpaInferenceEvent(CpaInferenceEventNotification {
                        request_id: params.request_id.clone(),
                        event,
                    }),
                )
                .await
            {
                return Err(failure(/*status*/ 499, "Connection closed"));
            }
        }
        if !terminal {
            return Err(failure(
                /*status*/ 502,
                "Upstream closed without a terminal event",
            ));
        }
        Ok(())
    }
}

fn failure(status: u16, message: &str) -> JSONRPCErrorError {
    JSONRPCErrorError {
        code: -32000,
        message: message.into(),
        data: Some(json!({"httpStatus": status})),
    }
}

fn api_failure(error: ApiError) -> JSONRPCErrorError {
    let (status, message) = match error {
        ApiError::Transport(TransportError::Http { status, .. }) | ApiError::Api { status, .. } => {
            (status.as_u16(), "Upstream inference failed")
        }
        ApiError::Transport(TransportError::Policy(_)) => (502, "Upstream transport policy denied"),
        ApiError::Transport(TransportError::Build(_)) => {
            (502, "Unable to prepare upstream request")
        }
        ApiError::Transport(
            TransportError::Connection(_) | TransportError::Network(_) | TransportError::Timeout,
        ) => (502, "Upstream network failure"),
        _ => (502, "Upstream inference failed"),
    };
    failure(status, message)
}

fn validate(params: &CpaInferenceStartParams) -> BridgeResult<()> {
    for value in [
        &params.request_id,
        &params.credential_id,
        &params.account_id,
        &params.source_format,
        &params.session_id,
    ] {
        if value.is_empty() || value.len() > 256 || value.chars().any(char::is_control) {
            return Err(failure(/*status*/ 400, "Invalid request identity"));
        }
    }
    if params.operation != "responses" {
        return Err(failure(
            /*status*/ 400,
            "Unsupported operation; only responses is available",
        ));
    }
    let r = &params.request;
    if r.contains_key("previous_response_id")
        || r.contains_key("conversation")
        || r.contains_key("generate")
        || r.get("store").is_some_and(|v| v != &Value::Bool(false))
        || r.get("background")
            .is_some_and(|v| v != &Value::Bool(false))
    {
        return Err(failure(
            /*status*/ 400,
            "Persistent sessions, generation control, storage and background responses are unsupported",
        ));
    }
    if r.get("stream") != Some(&Value::Bool(true)) {
        return Err(failure(/*status*/ 400, "stream=true is required"));
    }
    if let Some(tools) = r.get("tools") {
        let tools = tools
            .as_array()
            .ok_or_else(|| failure(/*status*/ 400, "tools must be an array"))?;
        if tools
            .iter()
            .any(|tool| !matches!(tool["type"].as_str(), Some("function" | "custom")))
        {
            return Err(failure(
                /*status*/ 400,
                "Only caller-executed function/custom tools are supported",
            ));
        }
    }
    // Callers supply Responses semantics, never transport/auth/installation metadata.
    for key in [
        "headers",
        "authorization",
        "Authorization",
        "cookies",
        "client_metadata",
        "installation_id",
        "account_id",
    ] {
        if r.contains_key(key) {
            return Err(failure(
                /*status*/ 400,
                "Caller transport or identity metadata is unsupported",
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "cpa_bridge_tests.rs"]
mod tests;
