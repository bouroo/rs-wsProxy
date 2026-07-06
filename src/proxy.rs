use axum::extract::ws::{Message, WebSocket};
use bytes::BytesMut;
use futures_util::{SinkExt, StreamExt};
use tokio::io::{self, AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

/// Connect to `addr` (force IPv4), enable no-delay, zero timeout.
///
/// `addr` may be a literal IP (`127.0.0.1:6900`) or a hostname (`login:6900`);
/// `tokio::net::lookup_host` resolves DNS when needed.
pub async fn connect_tcp(addr: &str) -> Result<TcpStream, String> {
    // Resolve the address, keeping only the first IPv4 result.
    let addrs = tokio::net::lookup_host(addr)
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
                Err(e) => {
                    tracing::error!("ws read error: {}", e);
                    break;
                }
                // Text/ping/pong are ignored to keep the proxy transparent to the game protocol.
                _ => {}
            }
        }
    };

    // TCP → WS: binary passthrough. Use BytesMut + split().freeze() so each
    // WebSocket frame references the buffer without a per-read allocation.
    let tcp_to_ws = async {
        let mut buf = BytesMut::with_capacity(65536); // 64 KiB chunk buffer.
        loop {
            // read_buf on an empty BytesMut only reserves 64 bytes, which would
            // cause an allocation storm and cap reads to 64 bytes. Keep the
            // buffer reasonably full so we can read large chunks.
            if buf.capacity() < 4096 {
                buf.reserve(65536);
            }
            match tcp_read.read_buf(&mut buf).await {
                Ok(0) => {
                    // TCP server closed the connection. Send a Close frame so
                    // the WebSocket client sees a graceful closure instead of
                    // an abrupt 1006 abnormal close.
                    let _ = ws_tx.send(Message::Close(None)).await;
                    break;
                }
                Ok(_) => {
                    let data = buf.split().freeze();
                    if ws_tx.send(Message::Binary(data)).await.is_err() {
                        break; // WS closed
                    }
                }
                Err(e) => {
                    tracing::error!("tcp→ws read error: {}", e);
                    break;
                }
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
    use std::net::SocketAddr;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    /// Echo TCP server — reads bytes and writes them back.
    /// Returns the join handle and the bound address.
    async fn echo_server(addr: SocketAddr) -> (tokio::task::JoinHandle<()>, SocketAddr) {
        let listener = TcpListener::bind(addr).await.unwrap();
        let bound_addr = listener.local_addr().unwrap();
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
        (handle, bound_addr)
    }

    #[tokio::test]
    async fn test_connect_tcp_to_echo() {
        let addr: SocketAddr = "127.0.0.1:0".parse().unwrap();
        let (handle, echo_addr) = echo_server(addr).await;

        // Connect and exchange data
        let mut stream = connect_tcp(&echo_addr.to_string()).await.unwrap();
        stream.write_all(b"hello").await.unwrap();

        let mut buf = vec![0u8; 16];
        let n = stream.read(&mut buf).await.unwrap();
        assert_eq!(&buf[..n], b"hello");

        handle.abort();
    }

    #[tokio::test]
    async fn test_connect_tcp_to_hostname() {
        // Verify that a hostname:port is resolved and connected successfully.
        let addr: SocketAddr = "127.0.0.1:0".parse().unwrap();
        let (handle, echo_addr) = echo_server(addr).await;

        let host = format!("localhost:{}", echo_addr.port());
        let mut stream = connect_tcp(&host).await.unwrap();
        stream.write_all(b"hostname").await.unwrap();

        let mut buf = vec![0u8; 16];
        let n = stream.read(&mut buf).await.unwrap();
        assert_eq!(&buf[..n], b"hostname");

        handle.abort();
    }

    #[tokio::test]
    async fn test_connect_tcp_rejects_invalid_address() {
        let result = connect_tcp("not-an-address").await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.contains("DNS lookup failed") || err.contains("no IPv4 address found"),
            "unexpected error: {}",
            err
        );
    }

    #[tokio::test]
    async fn test_connect_tcp_rejects_unreachable_port() {
        // Bind to an ephemeral port and drop it immediately to guarantee it is closed.
        // This avoids potential hangs in firewalled environments that drop packets to port 1.
        let port = {
            let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
            listener.local_addr().unwrap().port()
        };
        let result = connect_tcp(&format!("127.0.0.1:{}", port)).await;
        assert!(result.is_err());
    }
}
