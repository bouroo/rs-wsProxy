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
pub async fn handle_socket(socket: WebSocket, target: &str) {
    let tcp = match connect_tcp(target).await {
        Ok(s) => s,
        Err(e) => {
            tracing::error!("failed to connect TCP to {}: {}", target, e);
            return;
        }
    };

    let (mut ws_tx, mut ws_rx) = socket.split();
    let (mut tcp_read, mut tcp_write) = io::split(tcp);

    // WS → TCP
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
                _ => {} // ignore text/ping/pong
            }
        }
    };

    // TCP → WS (binary passthrough)
    let tcp_to_ws = async {
        let mut buf = vec![0u8; 65536]; // 64 KiB buffer
        loop {
            let n = match tcp_read.read(&mut buf).await {
                Ok(0) => break, // EOF
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

    /// Echo TCP server — reads bytes and writes them back.
    async fn echo_server(addr: SocketAddr) -> tokio::task::JoinHandle<()> {
        let listener = TcpListener::bind(addr).await.unwrap();
        tokio::spawn(async move {
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
        })
    }

    #[tokio::test]
    async fn test_connect_tcp_to_echo() {
        let addr: SocketAddr = "127.0.0.1:0".parse().unwrap();
        let handle = echo_server(addr).await;

        // Find actual bound port
        use std::net::UdpSocket;
        let udp = UdpSocket::bind("127.0.0.1:0").unwrap();
        let local = udp.local_addr().unwrap();

        // Use the actual listener address, not 0
        let listen_addr = "127.0.0.1:0".parse::<SocketAddr>().unwrap();
        let _listener = TcpListener::bind(listen_addr).await.unwrap();

        // Actually bind a real address — use the listener's local addr
        let actual_addr = _listener.local_addr().unwrap();

        // We need a fresh approach — bind the echo server and get its port
        handle.abort();

        // Re-bind with a specific port from the OS
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let echo_addr = listener.local_addr().unwrap();

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

        let stream = connect_tcp(&echo_addr.to_string()).await.unwrap();
        stream.write_all(b"hello").await.unwrap();

        let mut buf = vec![0u8; 16];
        let n = stream.read(&mut buf).await.unwrap();
        assert_eq!(&buf[..n], b"hello");

        handle.abort();
    }
}
