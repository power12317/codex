use super::images_tests::image_request;
use super::*;
use base64::Engine;
use pretty_assertions::assert_eq;
use test_case::test_case;
use tokio::io::AsyncReadExt;
use tokio::io::AsyncWriteExt;

#[test_case(true; "cancel_after_partial_image")]
#[test_case(false; "truncated_upstream_stream")]
#[tokio::test]
async fn cpa_images_partial_stream_errors_on_cancel_or_transport_failure(
    cancel: bool,
) -> Result<()> {
    let upstream = MockServer::start().await;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let base = format!("http://{}", listener.local_addr()?);
    let partial = "data: {\"type\":\"image_generation.partial_image\",\"b64_json\":\"AAH/\"}\n\n";
    let (close_tx, close_rx) = tokio::sync::oneshot::channel();
    let serving = tokio::spawn(async move {
        let mut byte = [0u8; 1];
        let (mut socket, head) = loop {
            let (mut socket, _) = listener.accept().await?;
            let mut request = Vec::new();
            while !request.ends_with(b"\r\n\r\n") {
                socket.read_exact(&mut byte).await?;
                request.push(byte[0]);
            }
            let head = String::from_utf8(request)?;
            if head.starts_with("GET ") {
                socket
                    .write_all(
                        b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                    )
                    .await?;
                continue;
            }
            break (socket, head);
        };
        assert!(head.starts_with("POST /v1/images/generations "));
        assert!(
            head.to_ascii_lowercase()
                .contains("authorization: bearer fake-cpa-access")
        );
        let length: usize = head
            .lines()
            .find_map(|line| {
                line.to_ascii_lowercase()
                    .strip_prefix("content-length:")
                    .map(|value| value.trim().parse().expect("length"))
            })
            .expect("content length");
        let mut body = vec![0; length];
        socket.read_exact(&mut body).await?;
        assert_eq!(
            serde_json::from_slice::<Value>(&body)?,
            json!({"model":"gpt-image-1.5", "prompt":"draw", "stream":true})
        );
        socket.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n").await?;
        socket
            .write_all(format!("{:x}\r\n{partial}\r\n", partial.len()).as_bytes())
            .await?;
        let _ = close_rx.await;
        if cancel {
            assert_eq!(
                timeout(Duration::from_secs(10), socket.read(&mut byte)).await??,
                0
            );
        }
        Ok::<_, anyhow::Error>(())
    });
    let home = TempDir::new()?;
    prepare_account(&upstream, &home)?;
    let config = home.path().join("state/shared.json/config.toml");
    let source = std::fs::read_to_string(&config)?;
    std::fs::write(&config, source.replace(&upstream.uri(), &base))?;
    let _runtime = launch(&upstream, &home).await?;
    let mut ws = connect(&home).await?;
    let mut request = image_request(
        "images/generations",
        "images",
        json!({"model":"gpt-image-1.5", "prompt":"draw", "stream":true}),
    );
    let request_id = "019a0000-0000-7000-8000-000000000001";
    request["params"]["requestId"] = json!(request_id);
    send(&mut ws, request).await?;
    let accepted = receive(&mut ws, "id", json!(2)).await?;
    assert_eq!(accepted["result"]["statusCode"], json!(200));
    let event = receive(&mut ws, "method", json!("cpa/inference/body")).await?;
    assert_eq!(event["params"]["requestId"], json!(request_id));
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(event["params"]["bodyBase64"].as_str().expect("body"))?;
    assert_eq!(bytes, partial.as_bytes());
    if cancel {
        send(
            &mut ws,
            json!({"id":5,"method":"cpa/inference/cancel","params":{"requestId":request_id}}),
        )
        .await?;
    }
    let _ = close_tx.send(());
    let terminal = timeout(Duration::from_secs(15), async {
        loop {
            let Some(Ok(Message::Text(text))) = ws.next().await else {
                anyhow::bail!("closed before error")
            };
            let message: Value = serde_json::from_str(&text)?;
            assert_ne!(message["method"], json!("cpa/inference/completed"));
            if message["method"] == "cpa/inference/error" {
                break Ok::<_, anyhow::Error>(message);
            }
        }
    })
    .await??;
    assert_eq!(terminal["params"]["requestId"], json!(request_id));
    assert_eq!(
        terminal["params"]["httpStatus"],
        json!(if cancel { 499 } else { 502 })
    );
    serving.await??;
    Ok(())
}
