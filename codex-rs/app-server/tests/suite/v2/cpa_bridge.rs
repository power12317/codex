//! Unix control-socket contract tests use fake managed credentials and a mock upstream.
#![cfg(unix)]
use anyhow::Result;
use app_test_support::ChatGptAuthFixture;
use app_test_support::MockResponsesConfig;
use app_test_support::TestAppServer;
use app_test_support::write_chatgpt_auth;
use codex_config::types::AuthCredentialsStoreMode;
use futures::SinkExt;
use futures::StreamExt;
use pretty_assertions::assert_eq;
use serde_json::Value;
use serde_json::json;
use tempfile::TempDir;
use tokio::net::UnixStream;
use tokio::time::Duration;
use tokio::time::timeout;
use tokio_tungstenite::WebSocketStream;
use tokio_tungstenite::client_async;
use tokio_tungstenite::tungstenite::Message;
use wiremock::Mock;
use wiremock::MockServer;
use wiremock::ResponseTemplate;
use wiremock::matchers::method;
use wiremock::matchers::path;

type Socket = WebSocketStream<UnixStream>;

async fn connect(home: &TempDir) -> Result<Socket> {
    let socket = home.path().join("cpa.sock");
    let stream = timeout(Duration::from_secs(/*secs*/ 30), async {
        loop {
            if let Ok(stream) = UnixStream::connect(&socket).await {
                break stream;
            }
            tokio::time::sleep(Duration::from_millis(/*millis*/ 20)).await;
        }
    })
    .await?;
    let (mut ws, _) = client_async("ws://localhost/", stream).await?;
    send(&mut ws, json!({"id":0,"method":"initialize","params":{"clientInfo":{"name":"cpa-tests","version":"1"},"capabilities":{"experimentalApi":true}}})).await?;
    let initialized = receive(&mut ws, "id", json!(0)).await?;
    assert!(initialized.get("result").is_some(), "{initialized}");
    send(&mut ws, json!({"method":"initialized"})).await?;
    Ok(ws)
}

async fn send(ws: &mut Socket, value: Value) -> Result<()> {
    ws.send(Message::Text(value.to_string().into())).await?;
    Ok(())
}

async fn receive(ws: &mut Socket, key: &str, expected: Value) -> Result<Value> {
    timeout(Duration::from_secs(/*secs*/ 30), async {
        while let Some(message) = ws.next().await {
            let message = message?;
            if let Message::Text(text) = message {
                let value: Value = serde_json::from_str(&text)?;
                if value[key] == expected {
                    return Ok(value);
                }
            }
        }
        anyhow::bail!("socket closed before expected message")
    })
    .await?
}

async fn server(upstream: &MockServer, home: &TempDir) -> Result<TestAppServer> {
    MockResponsesConfig::new(&upstream.uri())
        .with_provider_name("OpenAI")
        .with_provider_config("requires_openai_auth = true\nstream_idle_timeout_ms = 10")
        .with_root_config(&format!(
            "chatgpt_base_url = {:?}\ncli_auth_credentials_store = \"file\"",
            upstream.uri()
        ))
        .with_extra_config("[cpa_bridge]\nenabled = true\ncredential_id = \"worker\"")
        .write(home.path())?;
    write_chatgpt_auth(
        home.path(),
        ChatGptAuthFixture::new("fake-cpa-access")
            .account_id("account")
            .chatgpt_account_id("account")
            .chatgpt_user_id("fixture-user"),
        AuthCredentialsStoreMode::File,
    )?;
    let socket = format!("unix://{}", home.path().join("cpa.sock").display());
    TestAppServer::builder()
        .with_codex_home(home.path())
        .with_args(&["--listen", &socket])
        .with_env_overrides(&[(
            "CODEX_REFRESH_TOKEN_URL_OVERRIDE",
            Some(&format!("{}/oauth/token", upstream.uri())),
        )])
        .build()
        .await
}

fn start(request: Value) -> Value {
    json!({"id":2,"method":"cpa/inference/start","params":{"requestId":"r","credentialId":"worker","accountId":"account","operation":"responses","sourceFormat":"openai-response","sessionId":"caller-worker-account","request":request}})
}

#[tokio::test]
async fn cpa_lossless_single_inference_and_identity_contract() -> Result<()> {
    let upstream = MockServer::start().await;
    let home = TempDir::new()?;
    let marker = home.path().join("tool-must-not-run");
    let request = json!({"model":"mock-model","stream":true,"instructions":"keep exactly", "input":[], "tools":[{"type":"function","name":"shell","parameters":{"type":"object"}}], "tool_choice":{"type":"function","name":"shell"}, "future_extension":{"untouched":true}});
    let events = vec![
        json!({"type":"response.created","response":{"id":"resp"}}),
        json!({"type":"future.event","payload":{"untouched":true}}),
        json!({"type":"response.output_item.done","item":{"type":"function_call","call_id":"call","name":"shell","arguments":format!("{{\"command\":\"touch {}\"}}",marker.display())}}),
        json!({"type":"response.completed","response":{"id":"resp","output":[],"usage":{"input_tokens":1,"output_tokens":2,"total_tokens":3,"future_usage":7}}}),
    ];
    let body = events
        .iter()
        .map(|e| format!("data: {e}\n\n"))
        .collect::<String>();
    Mock::given(method("POST"))
        .and(path("/v1/responses"))
        .respond_with(
            ResponseTemplate::new(/*s*/ 200)
                .insert_header("content-type", "text/event-stream")
                .insert_header("set-cookie", "secret=never-forward")
                .set_body_raw(body, "text/event-stream"),
        )
        .expect(/*r*/ 1)
        .mount(&upstream)
        .await;
    let _runtime = server(&upstream, &home).await?;
    let mut ws = connect(&home).await?;
    send(
        &mut ws,
        json!({"id":1,"method":"cpa/capabilities/read","params":{}}),
    )
    .await?;
    let caps = receive(&mut ws, "id", json!(1)).await?;
    assert_eq!(caps["result"]["accountId"], json!("account"));
    assert_eq!(caps["result"]["persistentSessions"], json!(false));
    let mut wrong = start(request.clone());
    wrong["params"]["accountId"] = json!("wrong");
    send(&mut ws, wrong).await?;
    assert_eq!(
        receive(&mut ws, "id", json!(2)).await?["error"]["data"],
        json!({"httpStatus":403})
    );
    send(&mut ws, start(request.clone())).await?;
    let accepted = receive(&mut ws, "id", json!(2)).await?;
    assert_eq!(
        accepted["result"],
        json!({"requestId":"r","statusCode":200,"headers":{"content-type":["text/event-stream"]}})
    );
    let mut actual = Vec::new();
    for _ in &events {
        actual.push(
            receive(&mut ws, "method", json!("cpa/inference/event")).await?["params"]["event"]
                .clone(),
        );
    }
    assert_eq!(actual, events);
    assert_eq!(
        receive(&mut ws, "method", json!("cpa/inference/completed")).await?["params"],
        json!({"requestId":"r"})
    );
    assert!(!marker.exists());
    let requests = upstream.received_requests().await.unwrap();
    let inference: Vec<_> = requests
        .iter()
        .filter(|r| r.url.path() == "/v1/responses")
        .collect();
    assert_eq!(inference.len(), 1);
    assert_eq!(inference[0].body_json::<Value>()?, request);
    assert_eq!(
        inference[0].headers.get("authorization").unwrap(),
        "Bearer fake-cpa-access"
    );
    assert_eq!(
        inference[0].headers.get("chatgpt-account-id").unwrap(),
        "account"
    );
    ws.close(None).await?;
    let mut ws = connect(&home).await?;
    send(
        &mut ws,
        json!({"id":3,"method":"cpa/capabilities/read","params":{}}),
    )
    .await?;
    assert_eq!(
        receive(&mut ws, "id", json!(3)).await?["result"],
        caps["result"]
    );
    Ok(())
}

#[tokio::test]
async fn cpa_cancel_before_headers_and_disconnect_leave_runtime_alive() -> Result<()> {
    let upstream = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/responses"))
        .respond_with(
            ResponseTemplate::new(/*s*/ 200)
                .set_delay(Duration::from_secs(/*secs*/ 120))
                .set_body_string("unused"),
        )
        .mount(&upstream)
        .await;
    let home = TempDir::new()?;
    let _runtime = server(&upstream, &home).await?;
    let mut ws = connect(&home).await?;
    send(
        &mut ws,
        start(json!({"model":"mock-model","stream":true,"input":[]})),
    )
    .await?;
    // Wait for actual upstream admission before cancellation.
    timeout(Duration::from_secs(/*secs*/ 30), async {
        loop {
            if upstream
                .received_requests()
                .await
                .unwrap()
                .iter()
                .any(|r| r.url.path() == "/v1/responses")
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(/*millis*/ 10)).await;
        }
    })
    .await?;
    send(
        &mut ws,
        json!({"id":3,"method":"cpa/inference/cancel","params":{"requestId":"r"}}),
    )
    .await?;
    // Cancellation response and start error are independently queued.
    let mut replies = std::collections::BTreeMap::new();
    while replies.len() < 2 {
        if let Message::Text(text) = timeout(Duration::from_secs(/*secs*/ 10), ws.next())
            .await?
            .unwrap()?
        {
            let value: Value = serde_json::from_str(&text)?;
            if let Some(id) = value["id"].as_u64() {
                replies.insert(id, value);
            }
        }
    }
    assert_eq!(replies[&2]["error"]["data"], json!({"httpStatus":499}));
    assert_eq!(replies[&3]["result"], json!({}));
    ws.close(None).await?;
    let mut ws = connect(&home).await?;
    send(
        &mut ws,
        start(json!({"model":"mock-model","stream":true,"input":[]})),
    )
    .await?;
    ws.close(None).await?;
    let mut ws = connect(&home).await?;
    send(
        &mut ws,
        json!({"id":4,"method":"cpa/capabilities/read","params":{}}),
    )
    .await?;
    assert_eq!(
        receive(&mut ws, "id", json!(4)).await?["result"]["accountId"],
        json!("account")
    );
    Ok(())
}

#[tokio::test]
async fn cpa_managed_401_recovery_refreshes_native_storage() -> Result<()> {
    let upstream = MockServer::start().await;
    app_test_support::mount_workspace_routing(&upstream).await;
    let home = TempDir::new()?;
    let _runtime = server(&upstream, &home).await?;
    let auth: Value = serde_json::from_slice(&std::fs::read(home.path().join("auth.json"))?)?;
    Mock::given(method("POST")).and(path("/oauth/token"))
        .respond_with(ResponseTemplate::new(/*s*/ 200).set_body_json(json!({"access_token":"fake-refreshed", "refresh_token":"fake-refresh-next", "id_token":auth["tokens"]["id_token"]}))).expect(/*r*/ 1).mount(&upstream).await;
    Mock::given(method("POST"))
        .and(path("/v1/responses"))
        .and(wiremock::matchers::header(
            "authorization",
            "Bearer fake-cpa-access",
        ))
        .respond_with(ResponseTemplate::new(/*s*/ 401).set_body_string("private-error-never-echo"))
        .mount(&upstream)
        .await;
    Mock::given(method("POST")).and(path("/v1/responses")).and(wiremock::matchers::header("authorization","Bearer fake-refreshed"))
        .respond_with(ResponseTemplate::new(/*s*/ 200).insert_header("content-type","text/event-stream").set_body_string("data: {\"type\":\"response.completed\",\"response\":{\"id\":\"refreshed\"}}\n\n")).expect(/*r*/ 1).mount(&upstream).await;
    let mut ws = connect(&home).await?;
    send(
        &mut ws,
        start(json!({"model":"mock-model","stream":true,"input":[]})),
    )
    .await?;
    let accepted = receive(&mut ws, "id", json!(2)).await?;
    let calls: Vec<_> = upstream
        .received_requests()
        .await
        .unwrap()
        .iter()
        .map(|r| {
            (
                r.url.path().to_owned(),
                r.headers.get("authorization").is_some(),
            )
        })
        .collect();
    assert_eq!(
        accepted["result"]["statusCode"],
        json!(200),
        "{accepted}; calls={calls:?}"
    );
    receive(&mut ws, "method", json!("cpa/inference/completed")).await?;
    let saved: Value = serde_json::from_slice(&std::fs::read(home.path().join("auth.json"))?)?;
    assert_eq!(saved["tokens"]["access_token"], json!("fake-refreshed"));
    assert_eq!(saved["tokens"]["refresh_token"], json!("fake-refresh-next"));
    Ok(())
}

#[tokio::test]
async fn cpa_opt_in_and_signed_out_capabilities() -> Result<()> {
    for enabled in [false, true] {
        let home = TempDir::new()?;
        std::fs::write(
            home.path().join("config.toml"),
            format!(
                "cli_auth_credentials_store = \"file\"\n[cpa_bridge]\nenabled = {enabled}\ncredential_id = \"worker\"\n"
            ),
        )?;
        let socket = format!("unix://{}", home.path().join("cpa.sock").display());
        let _runtime = TestAppServer::builder()
            .with_codex_home(home.path())
            .with_args(&["--listen", &socket])
            .build()
            .await?;
        let mut ws = connect(&home).await?;
        send(
            &mut ws,
            json!({"id":1,"method":"cpa/capabilities/read","params":{}}),
        )
        .await?;
        let response = receive(&mut ws, "id", json!(1)).await?;
        if enabled {
            assert_eq!(
                (
                    response["result"]["accountId"].clone(),
                    response["result"]["authMode"].clone()
                ),
                (Value::Null, Value::Null)
            );
            send(
                &mut ws,
                start(json!({"model":"mock-model","stream":true,"input":[]})),
            )
            .await?;
            assert_eq!(
                receive(&mut ws, "id", json!(2)).await?["error"]["data"],
                json!({"httpStatus":401})
            );
        } else {
            assert_eq!(response["error"]["code"], json!(-32601));
        }
    }
    Ok(())
}

#[tokio::test]
async fn cpa_failures_never_replay_inference_or_echo_private_errors() -> Result<()> {
    for status in [503, 200] {
        let upstream = MockServer::start().await;
        let body = if status == 200 {
            "data: {\"type\":\"response.created\",\"response\":{\"id\":\"truncated\"}}\n\n"
        } else {
            "private error detail"
        };
        Mock::given(method("POST"))
            .and(path("/v1/responses"))
            .respond_with(ResponseTemplate::new(status).set_body_raw(body, "text/event-stream"))
            .expect(/*r*/ 1)
            .mount(&upstream)
            .await;
        let home = TempDir::new()?;
        let _runtime = server(&upstream, &home).await?;
        let mut ws = connect(&home).await?;
        send(
            &mut ws,
            start(json!({"model":"mock-model","stream":true,"input":[]})),
        )
        .await?;
        let accepted = receive(&mut ws, "id", json!(2)).await?;
        if status == 503 {
            assert_eq!(
                accepted["error"],
                json!({"code":-32000,"data":{"httpStatus":503},"message":"Upstream inference failed"})
            );
        } else {
            assert_eq!(accepted["result"]["statusCode"], json!(200));
            receive(&mut ws, "method", json!("cpa/inference/event")).await?;
            assert_eq!(
                receive(&mut ws, "method", json!("cpa/inference/error")).await?["params"],
                json!({"requestId":"r","httpStatus":502,"message":"Upstream inference failed"})
            );
            receive(&mut ws, "method", json!("cpa/inference/completed")).await?;
        }
    }
    Ok(())
}
