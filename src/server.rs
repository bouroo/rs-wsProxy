use axum::{
    extract::ws::{WebSocket, WebSocketUpgrade},
    response::{Html, Response},
    Router,
};
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::TcpListener;

use crate::config::AppState;
use crate::modules::{verify, VerifyResult};
use crate::proxy::handle_socket;

/// The HTTP-WS server — wraps Axum Router + AppState.
pub struct Server {
    router: Router<Arc<AppState>>,
}

impl Server {
    /// Build a new server with the given state.
    pub fn new(state: Arc<AppState>) -> Self {
        let router = Router::new()
            .route("/", axum::routing::get(get_root))
            .route("/:target", axum::routing::get(ws_upgrade));

        Server { router }
    }

    /// Start listening on the given port (plain HTTP/WebSocket).
    pub async fn start_plain(self, addr: SocketAddr) {
        tracing::info!("wsProxy listening on http://{}", addr);

        let listener = TcpListener::bind(addr).await.unwrap();
        axum::serve(listener, self.router.into_make_service())
            .await
            .unwrap();
    }

    /// Start listening with TLS (rustls). Caller must supply cert+key paths.
    pub async fn start_tls(self, addr: SocketAddr, cert_path: &str, key_path: &str) {
        tracing::info!("wsProxy listening on https://{} (TLS)", addr);

        let cert_file = std::fs::File::open(cert_path).unwrap();
        let mut reader = std::io::BufReader::new(cert_file);
        // Read certificates from PEM file
        let certs: Vec<rustls::Certificate> = rustls_pemfile::certs(&mut reader)
            .collect::<Result<Vec<_>, _>>()
            .unwrap();

        let key_file = std::fs::File::open(key_path).unwrap();
        let mut reader = std::io::BufReader::new(key_file);
        // Read private keys from the PEM file
        let key = rustls_pemfile::read_one(&mut reader)
            .ok()
            .flatten()
            .and_then(|item| match item {
                rustls_pemfile::Item::PKCS1Key(k) => Some(rustls::PrivateKey(k)),
                rustls_pemfile::Item::PKCS8Key(k) => Some(rustls::PrivateKey(k)),
                _ => None,
            })
            .expect("key file must contain a private key");

        let config = rustls::ServerConfig::builder()
            .with_safe_defaults()
            .with_no_client_auth()
            .with_single_cert(certs, key)
            .unwrap();

        let tls_listener = tokio_rustls::TlsAcceptor::from(Arc::new(config));

        // Use axum's TLS support via the `axum-server` crate.
        let listener = TcpListener::bind(addr).await.unwrap();
        axum_server::bind(listener)
            .tls(tls_listener)
            .serve(self.router.into_make_service())
            .await
            .unwrap();
    }
}

/// GET / — health check, returns "wsProxy running...\n" for compat.
async fn get_root() -> Html<String> {
    Html("wsProxy running...\n".to_string())
}

/// WebSocket upgrade handler — extracts target from URL path, runs verify pipeline, then proxies.
async fn ws_upgrade(
    ws: WebSocketUpgrade,
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    axum::extract::Path(target): axum::extract::Path<String>,
) -> Response {
    // Run verify pipeline (redirect → allow).
    match verify(&state, &target) {
        VerifyResult::Accepted(resolved_target) => {
            tracing::info!("ws upgrade: target={}", resolved_target);
            ws.on_upgrade(move |socket| handle_socket(socket, &resolved_target))
        }
        VerifyResult::Rejected(reason) => {
            tracing::warn!("ws rejected: {}", reason.0);
            ws.on_upgrade(|socket| async move {
                // Close with code 1008 (policy violation) and drop.
                if socket.close().await.is_ok() {
                    // closed
                }
            })
        }
    }
}
