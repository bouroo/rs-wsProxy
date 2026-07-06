use futures_util::{SinkExt, StreamExt};
use rs_ws_proxy::{AppState, Server};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio_tungstenite::tungstenite;

/// Start the plain HTTP/WebSocket server on an OS-assigned port and return the bound address.
async fn spawn_test_server(
    allowed: Vec<String>,
    redirects: HashMap<String, String>,
    default_target: Option<String>,
) -> SocketAddr {
    let state = Arc::new(AppState {
        allowed_servers: allowed,
        redirects,
        default_target,
    });
    let server = Server::new(state);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    tokio::spawn(async move {
        server.serve(listener).await;
    });

    tokio::time::sleep(Duration::from_millis(50)).await;
    addr
}

#[tokio::test]
async fn test_health_endpoint() {
    let addr = spawn_test_server(Vec::new(), HashMap::new(), None).await;
    let url = format!("http://{}/", addr);

    let client = reqwest::Client::new();
    let response = client
        .get(&url)
        .timeout(Duration::from_secs(5))
        .send()
        .await
        .unwrap();

    assert!(response.status().is_success());
    let body = response.text().await.unwrap();
    assert_eq!(body, "wsProxy running...\n");
}

#[tokio::test]
async fn test_ws_upgrade_rejected_for_invalid_target_format() {
    let addr = spawn_test_server(Vec::new(), HashMap::new(), None).await;
    let url = format!("ws://{}/not-a-valid-target", addr);

    let result = tokio_tungstenite::connect_async(&url).await;
    assert!(result.is_err(), "expected upgrade rejection");
}

#[tokio::test]
async fn test_ws_upgrade_rejected_when_not_in_allow_list() {
    let addr = spawn_test_server(vec!["127.0.0.1:6900".to_string()], HashMap::new(), None).await;
    let url = format!("ws://{}/127.0.0.1:5121", addr);

    let result = tokio_tungstenite::connect_async(&url).await;
    assert!(
        result.is_err(),
        "expected rejection for target not in allow list"
    );
}

#[tokio::test]
async fn test_route_matches_target_path() {
    let addr = spawn_test_server(vec!["abc".to_string()], HashMap::new(), None).await;
    let url = format!("http://{}/abc", addr);

    let client = reqwest::Client::new();
    let response = client
        .get(&url)
        .timeout(Duration::from_secs(5))
        .send()
        .await
        .unwrap();

    // Not a WS upgrade, but the route should match (not 404).
    assert_ne!(response.status().as_u16(), 404);
}

#[tokio::test]
async fn test_ws_upgrade_accepts_allowed_target() {
    let echo_addr = spawn_echo_tcp_server().await;
    let allowed = vec![echo_addr.to_string()];
    let proxy_addr = spawn_test_server(allowed, HashMap::new(), None).await;
    let url = format!("ws://{}/{}", proxy_addr, echo_addr);

    let (mut ws, _) = tokio_tungstenite::connect_async(&url).await.unwrap();

    let payload = vec![1, 2, 3, 4];
    ws.send(tungstenite::Message::Binary(payload.clone()))
        .await
        .unwrap();

    let response = ws.next().await.unwrap().unwrap();
    assert_eq!(response, tungstenite::Message::Binary(payload));
}

#[tokio::test]
async fn test_ws_default_route_without_default_target_is_rejected() {
    let addr = spawn_test_server(Vec::new(), HashMap::new(), None).await;
    let url = format!("ws://{}/ws", addr);

    let result = tokio_tungstenite::connect_async(&url).await;
    assert!(
        result.is_err(),
        "expected rejection when no /ws default target is configured"
    );
}

#[tokio::test]
async fn test_ws_default_route_proxies_via_redirect_fallback() {
    let echo_addr = spawn_echo_tcp_server().await;
    let mut redirects = HashMap::new();
    redirects.insert("ws".to_string(), echo_addr.to_string());
    let proxy_addr = spawn_test_server(Vec::new(), redirects, None).await;

    let url = format!("ws://{}/ws", proxy_addr);
    let (mut ws, _) = tokio_tungstenite::connect_async(&url).await.unwrap();

    let payload = vec![9, 10, 11, 12];
    ws.send(tungstenite::Message::Binary(payload.clone()))
        .await
        .unwrap();

    let response = ws.next().await.unwrap().unwrap();
    assert_eq!(response, tungstenite::Message::Binary(payload));
}

#[tokio::test]
async fn test_ws_default_route_proxies_via_default_target() {
    let echo_addr = spawn_echo_tcp_server().await;
    let allowed = vec![echo_addr.to_string()];
    let proxy_addr = spawn_test_server(allowed, HashMap::new(), Some(echo_addr.to_string())).await;

    let url = format!("ws://{}/ws", proxy_addr);
    let (mut ws, _) = tokio_tungstenite::connect_async(&url).await.unwrap();

    let payload = vec![21, 22, 23, 24];
    ws.send(tungstenite::Message::Binary(payload.clone()))
        .await
        .unwrap();

    let response = ws.next().await.unwrap().unwrap();
    assert_eq!(response, tungstenite::Message::Binary(payload));
}

#[tokio::test]
async fn test_ws_default_target_takes_priority_over_redirect_ws() {
    let echo_addr = spawn_echo_tcp_server().await;
    let mut redirects = HashMap::new();
    // This redirect should be ignored when default_target is set.
    redirects.insert("ws".to_string(), "127.0.0.1:1".to_string());
    let allowed = vec![echo_addr.to_string()];
    let proxy_addr = spawn_test_server(allowed, redirects, Some(echo_addr.to_string())).await;

    let url = format!("ws://{}/ws", proxy_addr);
    let (mut ws, _) = tokio_tungstenite::connect_async(&url).await.unwrap();

    let payload = vec![31, 32, 33, 34];
    ws.send(tungstenite::Message::Binary(payload.clone()))
        .await
        .unwrap();

    let response = ws.next().await.unwrap().unwrap();
    assert_eq!(response, tungstenite::Message::Binary(payload));
}

#[tokio::test]
async fn test_ws_default_route_respects_allow_list() {
    let echo_addr = spawn_echo_tcp_server().await;
    let allowed = vec!["some-other-host:1234".to_string()];
    let proxy_addr = spawn_test_server(allowed, HashMap::new(), Some(echo_addr.to_string())).await;

    let url = format!("ws://{}/ws", proxy_addr);
    let result = tokio_tungstenite::connect_async(&url).await;
    assert!(
        result.is_err(),
        "expected rejection because resolved target is not in allow list"
    );
}

#[tokio::test]
async fn test_ws_upgrade_redirect_rewrites_target() {
    let echo_addr = spawn_echo_tcp_server().await;
    let mut redirects = HashMap::new();
    redirects.insert("login:6900".to_string(), echo_addr.to_string());
    let proxy_addr = spawn_test_server(Vec::new(), redirects, None).await;

    // Target is redirected to the echo server, so the upgrade succeeds.
    let url = format!("ws://{}/login:6900", proxy_addr);
    let (mut ws, _) = tokio_tungstenite::connect_async(&url).await.unwrap();

    let payload = vec![5, 6, 7, 8];
    ws.send(tungstenite::Message::Binary(payload.clone()))
        .await
        .unwrap();

    let response = ws.next().await.unwrap().unwrap();
    assert_eq!(response, tungstenite::Message::Binary(payload));
}

#[tokio::test]
async fn test_robrowser_route_still_works() {
    let echo_addr = spawn_echo_tcp_server().await;
    let allowed = vec![echo_addr.to_string()];
    let proxy_addr = spawn_test_server(allowed, HashMap::new(), None).await;

    let url = format!("ws://{}/{}", proxy_addr, echo_addr);
    let (mut ws, _) = tokio_tungstenite::connect_async(&url).await.unwrap();

    let payload = vec![1, 2, 3, 4];
    ws.send(tungstenite::Message::Binary(payload.clone()))
        .await
        .unwrap();

    let response = ws.next().await.unwrap().unwrap();
    assert_eq!(response, tungstenite::Message::Binary(payload));
}

/// Spawn a TCP echo server on an OS-assigned port and return its address.
async fn spawn_echo_tcp_server() -> SocketAddr {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    tokio::spawn(async move {
        if let Ok((stream, _)) = listener.accept().await {
            let (mut read, mut write) = tokio::io::split(stream);
            let mut buf = vec![0u8; 65536];
            loop {
                let n = match read.read(&mut buf).await {
                    Ok(0) => break,
                    Ok(n) => n,
                    Err(_) => break,
                };
                if write.write_all(&buf[..n]).await.is_err() {
                    break;
                }
            }
        }
    });

    addr
}
