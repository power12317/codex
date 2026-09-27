//! CPA TCP contract tests use a shared fake credential file and a mock upstream.
use anyhow::Result;
use app_test_support::ChatGptIdTokenClaims;
use app_test_support::MockResponsesConfig;
use app_test_support::TestAppServer;
use app_test_support::encode_id_token;
use futures::SinkExt;
use futures::StreamExt;
use pretty_assertions::assert_eq;
use serde_json::Value;
use serde_json::json;
use tempfile::TempDir;
use tokio::net::TcpStream;
use tokio::time::Duration;
use tokio::time::timeout;
use tokio_tungstenite::WebSocketStream;
use tokio_tungstenite::client_async;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use wiremock::Mock;
use wiremock::MockServer;
use wiremock::ResponseTemplate;
use wiremock::matchers::method;
use wiremock::matchers::path;

type Socket = WebSocketStream<TcpStream>;

async fn connect(home: &TempDir) -> Result<Socket> {
    let port = std::fs::read_to_string(home.path().join("port"))?;
    let socket = format!("127.0.0.1:{port}");
    let stream = timeout(Duration::from_secs(/*secs*/ 30), async {
        loop {
            if let Ok(stream) = TcpStream::connect(&socket).await {
                break stream;
            }
            tokio::time::sleep(Duration::from_millis(/*millis*/ 20)).await;
        }
    })
    .await?;
    let mut request = format!("ws://{socket}/cpa/v1/ws").into_client_request()?;
    request
        .headers_mut()
        .insert("authorization", "Bearer test-bridge-key".parse()?);
    let (mut ws, _) = client_async(request, stream).await?;
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
        .write(home.path())?;
    let id_token = encode_id_token(
        &ChatGptIdTokenClaims::new()
            .chatgpt_account_id("account")
            .chatgpt_user_id("fixture-user")
            .email("fixture@example.com"),
    )?;
    std::fs::write(
        home.path().join("shared.json"),
        serde_json::to_vec(&json!({
            "type":"codex", "id_token":id_token, "access_token":"fake-cpa-access",
            "refresh_token":"refresh-token", "account_id":"account", "email":"fixture@example.com",
            "last_refresh":chrono::Utc::now(), "expired":"2099-01-01T00:00:00Z",
            "unknown_field":{"keep":true}, "codex_cli":{"enabled":true,"worker_id":"worker","owner":"codex"}
        }))?,
    )?;
    launch(upstream, home).await
}

async fn launch(upstream: &MockServer, home: &TempDir) -> Result<TestAppServer> {
    let listener = std::net::TcpListener::bind("127.0.0.1:0")?;
    let port = listener.local_addr()?.port().to_string();
    drop(listener);
    std::fs::write(home.path().join("port"), &port)?;
    TestAppServer::builder()
        .with_codex_home(home.path())
        .with_env_overrides(&[
            (
                "CODEX_CPA_AUTH_FILE",
                Some(&home.path().join("shared.json").display().to_string()),
            ),
            ("CODEX_CPA_WORKER_ID", Some("worker")),
            ("CODEX_CPA_BRIDGE_KEY", Some("test-bridge-key")),
            ("CODEX_CPA_PORT", Some(&port)),
            ("CODEX_APP_SERVER_LOGIN_ISSUER", Some(&upstream.uri())),
            (
                "CODEX_REFRESH_TOKEN_URL_OVERRIDE",
                Some(&format!("{}/oauth/token", upstream.uri())),
            ),
        ])
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
    ws.close(/*msg*/ None).await?;
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
    ws.close(/*msg*/ None).await?;
    let mut ws = connect(&home).await?;
    send(
        &mut ws,
        start(json!({"model":"mock-model","stream":true,"input":[]})),
    )
    .await?;
    ws.close(/*msg*/ None).await?;
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
async fn cpa_managed_401_recovery_refreshes_shared_storage() -> Result<()> {
    let upstream = MockServer::start().await;
    app_test_support::mount_workspace_routing(&upstream).await;
    let home = TempDir::new()?;
    let _runtime = server(&upstream, &home).await?;
    let auth: Value = serde_json::from_slice(&std::fs::read(home.path().join("shared.json"))?)?;
    Mock::given(method("POST")).and(path("/oauth/token"))
        .respond_with(ResponseTemplate::new(/*s*/ 200).set_body_json(json!({"access_token":"fake-refreshed", "refresh_token":"fake-refresh-next", "id_token":auth["id_token"]}))).expect(/*r*/ 1).mount(&upstream).await;
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
    let saved: Value = serde_json::from_slice(&std::fs::read(home.path().join("shared.json"))?)?;
    assert_eq!(saved["access_token"], json!("fake-refreshed"));
    assert_eq!(saved["refresh_token"], json!("fake-refresh-next"));
    assert_eq!(saved["unknown_field"], json!({"keep":true}));
    assert!(!home.path().join("auth.json").exists());
    Ok(())
}

#[tokio::test]
async fn cpa_signed_out_login_owner_switch_and_whitelist() -> Result<()> {
    let upstream = MockServer::start().await;
    let home = TempDir::new()?;
    let _runtime = server(&upstream, &home).await?;
    let file = home.path().join("shared.json");
    let mut credential: Value = serde_json::from_slice(&std::fs::read(&file)?)?;
    let id_token = credential["id_token"].clone();
    credential["access_token"] = json!("");
    std::fs::write(&file, serde_json::to_vec(&credential)?)?;
    let mut ws = connect(&home).await?;
    send(
        &mut ws,
        json!({"id":10,"method":"cpa/credential/reload","params":{}}),
    )
    .await?;
    let caps = receive(&mut ws, "id", json!(10)).await?;
    assert_eq!(
        (
            caps["result"]["protocolVersion"].clone(),
            caps["result"]["credentialFile"].clone(),
            caps["result"]["authOwner"].clone(),
            caps["result"]["manualOAuth"].clone(),
            caps["result"]["authMode"].clone()
        ),
        (
            json!(2),
            json!("shared.json"),
            json!("codex"),
            json!(true),
            Value::Null
        )
    );
    send(&mut ws, json!({"id":11,"method":"thread/list","params":{}})).await?;
    assert_eq!(
        receive(&mut ws, "id", json!(11)).await?["error"]["code"],
        json!(-32601)
    );
    send(
        &mut ws,
        json!({"id":12,"method":"cpa/auth/login/start","params":{}}),
    )
    .await?;
    let login = receive(&mut ws, "id", json!(12)).await?["result"].clone();
    ws.close(/*msg*/ None).await?;
    ws = connect(&home).await?;
    let auth_url = url::Url::parse(login["authUrl"].as_str().unwrap())?;
    let query: std::collections::HashMap<_, _> = auth_url.query_pairs().into_owned().collect();
    assert_eq!(
        (&query["response_type"], &query["code_challenge_method"]),
        (&"code".to_string(), &"S256".to_string())
    );
    Mock::given(method("POST"))
        .and(path("/oauth/token"))
        .respond_with(ResponseTemplate::new(/*s*/ 200).set_body_json(
            json!({"id_token":id_token,"access_token":"logged-in","refresh_token":"new-refresh"}),
        ))
        .expect(/*r*/ 1)
        .mount(&upstream)
        .await;
    send(
        &mut ws,
        json!({"id":13,"method":"cpa/auth/login/status","params":{"loginId":login["loginId"]}}),
    )
    .await?;
    assert_eq!(
        receive(&mut ws, "id", json!(13)).await?["result"],
        json!({"status":"pending","error":null})
    );
    send(&mut ws,json!({"id":14,"method":"cpa/auth/login/callback","params":{"loginId":login["loginId"],"redirectUrl":format!("{}?code=test-code&state={}",query["redirect_uri"],query["state"])}})).await?;
    assert_eq!(
        receive(&mut ws, "id", json!(14)).await?["result"],
        json!({"status":"completed","error":null})
    );
    let saved: Value = serde_json::from_slice(&std::fs::read(&file)?)?;
    assert_eq!(
        (
            saved["access_token"].clone(),
            saved["unknown_field"].clone(),
            saved["codex_cli"].clone()
        ),
        (
            json!("logged-in"),
            credential["unknown_field"].clone(),
            credential["codex_cli"].clone()
        )
    );
    assert!(!home.path().join("auth.json").exists());
    let calls = upstream.received_requests().await.unwrap();
    let exchange = calls
        .iter()
        .find(|r| r.url.path() == "/oauth/token")
        .unwrap();
    let form: std::collections::HashMap<_, _> = url::form_urlencoded::parse(&exchange.body)
        .into_owned()
        .collect();
    use base64::Engine;
    use sha2::Digest;
    assert_eq!(
        base64::engine::general_purpose::URL_SAFE_NO_PAD
            .encode(sha2::Sha256::digest(form["code_verifier"].as_bytes())),
        query["code_challenge"]
    );
    credential = saved;
    credential["codex_cli"]["owner"] = json!("cpa");
    std::fs::write(&file, serde_json::to_vec(&credential)?)?;
    send(
        &mut ws,
        json!({"id":15,"method":"cpa/credential/reload","params":{}}),
    )
    .await?;
    let caps = receive(&mut ws, "id", json!(15)).await?;
    assert_eq!(
        (
            caps["result"]["authOwner"].clone(),
            caps["result"]["authMode"].clone()
        ),
        (json!("cpa"), Value::Null)
    );
    send(
        &mut ws,
        start(json!({"model":"mock-model","stream":true,"input":[]})),
    )
    .await?;
    assert_eq!(
        receive(&mut ws, "id", json!(2)).await?["error"]["data"],
        json!({"httpStatus":403})
    );
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

#[tokio::test]
async fn cpa_tcp_key_path_and_manual_callback_binding() -> Result<()> {
    let upstream = MockServer::start().await;
    let home = TempDir::new()?;
    let _runtime = server(&upstream, &home).await?;
    let mut ws = connect(&home).await?;
    let port = std::fs::read_to_string(home.path().join("port"))?;
    for (path, key, status) in [("/cpa/v1/ws", "wrong", 401), ("/", "test-bridge-key", 404)] {
        let stream = TcpStream::connect(format!("127.0.0.1:{port}")).await?;
        let mut request = format!("ws://127.0.0.1:{port}{path}").into_client_request()?;
        request
            .headers_mut()
            .insert("authorization", format!("Bearer {key}").parse()?);
        let err = client_async(request, stream).await.unwrap_err();
        let tokio_tungstenite::tungstenite::Error::Http(response) = err else {
            anyhow::bail!("unexpected handshake error")
        };
        assert_eq!(response.status().as_u16(), status);
    }
    for wrong_url in [false, true] {
        send(
            &mut ws,
            json!({"id":30,"method":"cpa/auth/login/start","params":{}}),
        )
        .await?;
        let login = receive(&mut ws, "id", json!(30)).await?["result"].clone();
        ws.close(/*msg*/ None).await?;
        ws = connect(&home).await?;
        let redirect = if wrong_url {
            format!(
                "{}/steal?code=c&state={}",
                upstream.uri(),
                login["state"].as_str().unwrap()
            )
        } else {
            "http://127.0.0.1:1455/auth/callback?code=c&state=wrong".into()
        };
        send(&mut ws,json!({"id":31,"method":"cpa/auth/login/callback","params":{"loginId":login["loginId"],"redirectUrl":redirect}})).await?;
        assert_eq!(
            receive(&mut ws, "id", json!(31)).await?["result"],
            json!({"status":"error","error":"OAuth login failed"})
        );
    }
    assert!(
        upstream
            .received_requests()
            .await
            .unwrap()
            .iter()
            .all(|r| !matches!(r.url.path(), "/steal" | "/oauth/token"))
    );
    Ok(())
}

#[tokio::test]
async fn cpa_owner_watcher_cancels_old_request_and_reloads_tokens() -> Result<()> {
    let upstream = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/responses"))
        .respond_with(ResponseTemplate::new(/*s*/ 200).set_delay(Duration::from_secs(/*secs*/ 120)))
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
            tokio::time::sleep(Duration::from_millis(/*millis*/ 20)).await;
        }
    })
    .await?;
    let file = home.path().join("shared.json");
    let mut value: Value = serde_json::from_slice(&std::fs::read(&file)?)?;
    value["codex_cli"]["owner"] = json!("cpa");
    std::fs::write(&file, serde_json::to_vec(&value)?)?;
    let response = receive(&mut ws, "id", json!(2)).await?;
    assert!([json!(401), json!(499)].contains(&response["error"]["data"]["httpStatus"]));
    value["codex_cli"]["owner"] = json!("codex");
    value["account_id"] = json!("next-account");
    std::fs::write(&file, serde_json::to_vec(&value)?)?;
    send(
        &mut ws,
        json!({"id":32,"method":"cpa/credential/reload","params":{}}),
    )
    .await?;
    assert_eq!(
        receive(&mut ws, "id", json!(32)).await?["result"]["accountId"],
        json!("next-account")
    );
    Ok(())
}
