use axum::extract::ws::{Message, WebSocket};
use futures_util::{SinkExt, StreamExt};
use std::net::SocketAddr;
use tokio::io::{self, AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

/// Connect to `addr` (force IPv4), enable no-delay, zero timeout.
pub async fn connect_tcp(addr: &str) -> Result<TcpStream, String> {
    let socket_addr: SocketAddr = addr
        .parse()
        .map_err(|e| format!("invalid target address '{}': {}", addr, e))?;

    // Force IPv4 — filter to v4 if DNS resolves multiple.
    let addrs = tokio::net::lookup_host(socket_addr)
        .await
        .map_err(|e| format!("DNS lookup failed for '{}': {}", addr, e))?
        .find(|a| a.is_ipv4())
        .ok_or_else(|| format!("no IPv4 address found for '{}'", addr))?;

    let stream = TcpStream::connect(addrs)
        .await
        .map_err(|e| format!("TCP connect failed for '{}': {}", addr, e))?;

    stream
        .set_nodelay(true)
        .map_err(|e| format!("set_nodelay failed: {}", e))?;

    Ok(stream)
}

/// Bidirectional pump between a WebSocket and a TCP stream.
/// Spawns two half-loops (ws→tcp, tcp→ws) and joins them.
pub async fn handle_socket(socket: WebSocket, target: String) {
    let tcp = match connect_tcp(&target).await {
        Ok(s) => s,
        Err(e) => {
            tracing::error!("failed to connect TCP to {}: {}", target, e);
            return;
        }
    };

    let (mut ws_tx, mut ws_rx) = socket.split();
    // Splitting the TCP stream lets us read and write concurrently without a mutex.
    let (mut tcp_read, mut tcp_write) = io::split(tcp);

    // WS → TCP: forward only binary frames; the roBrowser protocol uses binary payloads.
    let ws_to_tcp = async {
        while let Some(msg) = ws_rx.next().await {
            match msg {
                Ok(Message::Binary(data)) => {
                    if let Err(e) = tcp_write.write_all(&data).await {
                        tracing::error!("ws→tcp write error: {}", e);
                        break;
                    }
                }
                Ok(Message::Close(_)) => break,
                // Text/ping/pong are ignored to keep the proxy transparent to the game protocol.
                _ => {}
            }
        }
    };

    // TCP → WS: binary passthrough. Use a single reusable buffer to avoid per-read allocations.
    let tcp_to_ws = async {
        let mut buf = vec![0u8; 65536]; // 64 KiB matches a typical MTU-friendly chunk size.
        loop {
            let n = match tcp_read.read(&mut buf).await {
                Ok(0) => break, // EOF: the TCP server closed the connection.
                Ok(n) => n,
                Err(e) => {
                    tracing::error!("tcp→ws read error: {}", e);
                    break;
                }
            };

            if ws_tx
                .send(Message::Binary(buf[..n].to_vec()))
                .await
                .is_err()
            {
                break; // WS closed
            }
        }
    };

    // Run both half-loops concurrently; stop as soon as either side closes or errors.
    tokio::select! {
        _ = ws_to_tcp => {}
        _ = tcp_to_ws => {}
    }

    tracing::info!("proxy closed for target {}", target);
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    #[tokio::test]
    async fn test_connect_tcp_to_echo() {
        // Bind a listener on an OS-assigned port (port 0)
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let echo_addr = listener.local_addr().unwrap();

        // Spawn echo server in background
        let handle = tokio::spawn(async move {
            if let Ok((mut stream, _)) = listener.accept().await {
                let mut buf = vec![0u8; 65536];
                loop {
                    let n = match stream.read(&mut buf).await {
                        Ok(0) => break,
                        Ok(n) => n,
                        Err(_) => break,
                    };
                    if stream.write_all(&buf[..n]).await.is_err() {
                        break;
                    }
                }
            }
        });

        // Connect and exchange data
        let mut stream = connect_tcp(&echo_addr.to_string()).await.unwrap();
        stream.write_all(b"hello").await.unwrap();

        let mut buf = vec![0u8; 16];
        let n = stream.read(&mut buf).await.unwrap();
        assert_eq!(&buf[..n], b"hello");

        handle.abort();
    }

    #[tokio::test]
    async fn test_connect_tcp_rejects_invalid_address() {
        let result = connect_tcp("not-an-address").await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("invalid target address"));
    }

    #[tokio::test]
    async fn test_connect_tcp_rejects_unreachable_port() {
        // Nothing should be listening on this port; connect fails quickly.
        let result = connect_tcp("127.0.0.1:1").await;
        assert!(result.is_err());
    }
}
