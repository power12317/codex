//! Log the same prepared inference request that is handed to the HTTP transport.
use crate::outgoing_message::ConnectionId;
use crate::outgoing_message::OutgoingMessageSender;
use axum::http::HeaderMap;
use base64::Engine;
use codex_api::ReqwestTransport;
use codex_api::TransportError;
use codex_app_server_protocol::CpaInferenceUpstreamNotification;
use codex_app_server_protocol::ServerNotification;
use codex_http_client::HttpClientFactory;
use codex_http_client::HttpTransport;
use codex_http_client::Request;
use codex_http_client::Response;
use codex_http_client::StreamResponse;
use serde_json::Value;
use sha2::Digest;
use sha2::Sha256;
use std::collections::BTreeMap;
use std::io::Write;
use std::sync::Arc;

pub(super) struct UpstreamTransport {
    pub(super) inner: ReqwestTransport,
    pub(super) factory: HttpClientFactory,
    pub(super) outgoing: Arc<OutgoingMessageSender>,
    pub(super) connection_id: ConnectionId,
    pub(super) request_id: String,
    pub(super) credential_id: String,
}

impl UpstreamTransport {
    async fn notify(
        &self,
        notification: CpaInferenceUpstreamNotification,
    ) -> Result<(), TransportError> {
        // stderr is inherited by the master and remains visible with RUST_LOG unset.
        let mut details = serde_json::to_value(&notification).unwrap_or(Value::Null);
        if let Some(details) = details.as_object_mut() {
            // Full payloads remain in the existing upstream notification. Expose the
            // upstream structured failure so docker logs identifies invalid input too.
            if let Some(error) = details
                .get("body")
                .and_then(Value::as_str)
                .and_then(|body| serde_json::from_str::<Value>(body).ok())
                .and_then(|body| body.get("error").cloned())
            {
                details.insert(
                    "error".into(),
                    serde_json::json!({
                        "code": error["code"], "type": error["type"], "message": error["message"],
                    }),
                );
            }
            details.remove("body");
        }
        let event = if details["kind"] == "response" {
            "cpa.request.upstream.response"
        } else {
            "cpa.request.upstream"
        };
        record(&self.credential_id, &self.request_id, event, details);
        if self
            .outgoing
            .send_server_notification_to_connection_and_wait(
                self.connection_id,
                ServerNotification::CpaInferenceUpstream(notification),
            )
            .await
        {
            Ok(())
        } else {
            Err(TransportError::Network("CPA connection closed".into()))
        }
    }
}

impl HttpTransport for UpstreamTransport {
    async fn execute(&self, req: Request) -> Result<Response, TransportError> {
        self.inner.execute(req).await
    }

    async fn stream(&self, mut req: Request) -> Result<StreamResponse, TransportError> {
        // Defaults and the shared cookie jar are applied to the actual request,
        // rather than reconstructed separately for the log after dispatch.
        for (name, value) in &codex_login::default_client::default_headers() {
            req.headers.entry(name.clone()).or_insert(value.clone());
        }
        if !req.headers.contains_key(axum::http::header::COOKIE)
            && let Ok(uri) = req.url.parse()
            && let Some(cookie) = self.factory.chatgpt_cookie_header(&uri)
        {
            req.headers.insert(axum::http::header::COOKIE, cookie);
        }
        let prepared = req.prepare_body_for_send().map_err(TransportError::Build)?;
        let access_token_sha256 = req
            .headers
            .get(axum::http::header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "))
            .map(|token| format!("{:x}", Sha256::digest(token.as_bytes())));
        self.notify(CpaInferenceUpstreamNotification::Request {
            request_id: self.request_id.clone(),
            url: req.url.clone(),
            method: req.method.to_string(),
            headers: log_headers(&prepared.headers),
            body: String::from_utf8_lossy(&prepared.body_bytes()).into_owned(),
            access_token_sha256,
            oai_lb_node: oai_lb_node(&prepared.headers),
        })
        .await?;
        let result = self.inner.stream(req).await;
        let notification = match &result {
            Ok(response) => CpaInferenceUpstreamNotification::Response {
                request_id: self.request_id.clone(),
                status_code: response.status.as_u16(),
                headers: log_headers(&response.headers),
                body: None,
                oai_lb_node: oai_lb_node(&response.headers),
            },
            Err(TransportError::Http {
                status,
                headers,
                body,
                ..
            }) => CpaInferenceUpstreamNotification::Response {
                request_id: self.request_id.clone(),
                status_code: status.as_u16(),
                headers: headers.as_ref().map(log_headers).unwrap_or_default(),
                body: body.clone(),
                oai_lb_node: headers.as_ref().and_then(oai_lb_node),
            },
            Err(error) => CpaInferenceUpstreamNotification::Error {
                request_id: self.request_id.clone(),
                message: error.to_string(),
            },
        };
        self.notify(notification).await?;
        result
    }
}

pub(super) fn log_headers(headers: &HeaderMap) -> BTreeMap<String, Vec<String>> {
    headers
        .keys()
        .map(|name| {
            let sensitive = matches!(
                name.as_str(),
                "authorization" | "proxy-authorization" | "cookie" | "set-cookie"
            ) || ["authorization", "api-key", "apikey", "token", "secret"]
                .iter()
                .any(|part| name.as_str().contains(part));
            let values = headers
                .get_all(name)
                .iter()
                .map(|value| {
                    if sensitive {
                        "[REDACTED]".into()
                    } else {
                        String::from_utf8_lossy(value.as_bytes()).into_owned()
                    }
                })
                .collect();
            (name.to_string(), values)
        })
        .collect()
}

fn oai_lb_node(headers: &HeaderMap) -> Option<String> {
    let cookie = headers
        .get_all(axum::http::header::SET_COOKIE)
        .iter()
        .chain(headers.get_all(axum::http::header::COOKIE).iter())
        .filter_map(|value| value.to_str().ok())
        .flat_map(|value| value.split(';'))
        .find_map(|part| part.trim().strip_prefix("__oailb="))?;
    let payload = cookie.trim_matches('"').split('.').nth(/*n*/ 1)?;
    let payload = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(payload)
        .ok()?;
    let claims: Value = serde_json::from_slice(&payload).ok()?;
    let host = claims["host"]
        .as_str()?
        .trim()
        .trim_end_matches('.')
        .to_ascii_lowercase();
    let node = host
        .strip_prefix("chat.gateway.")?
        .strip_suffix(".api.openai.com")?;
    (!node.is_empty()
        && node.len() <= 63
        && !node.starts_with('-')
        && !node.ends_with('-')
        && node
            .bytes()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-'))
    .then(|| node.to_string())
}

/// JSONL to stderr only: stdout is reserved for JSON-RPC. Headers are already redacted.
pub(crate) fn record(credential_id: &str, request_id: &str, event: &str, details: Value) {
    let record = serde_json::json!({"timestamp": chrono::Utc::now().to_rfc3339(),
        "level": "INFO", "target": "codex_app_server::cpa_bridge",
        "fields": {"event.name": event, "credentialId": credential_id,
        "rpcRequestId": request_id, "details": details}});
    let _ = writeln!(std::io::stderr().lock(), "{record}");
}
