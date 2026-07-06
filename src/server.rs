use axum::{http::StatusCode, response::Html, response::IntoResponse, Router};
use std::net::SocketAddr;
use std::sync::Arc;

use crate::config::AppState;
use crate::modules::{validate_target, verify, VerifyResult};
use crate::proxy::handle_socket;

/// The HTTP-WS server — wraps Axum Router + AppState.
pub struct Server {
    router: Router,
}

impl Server {
    /// Build a new server with the given state.
    pub fn new(state: Arc<AppState>) -> Self {
        // Order matters: `/ws` must be matched before the catch-all `/:target`.
        let router = Router::new()
            .route("/", axum::routing::get(get_root))
            .route("/ws", axum::routing::get(ws_upgrade_default))
            .route("/:target", axum::routing::get(ws_upgrade))
            .with_state(state);

        Server { router }
    }

    /// Start listening on the given port (plain HTTP/WebSocket).
    pub async fn start_plain(self, addr: SocketAddr) {
        tracing::info!("wsProxy listening on http://{}", addr);

        let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
        self.serve(listener).await;
    }

    /// Serve an already-bound listener (used by tests and by `start_plain`).
    pub async fn serve(self, listener: tokio::net::TcpListener) {
        axum::serve(listener, self.router).await.unwrap();
    }

    /// Start listening with TLS (rustls). Caller must supply cert+key paths.
    pub async fn start_tls(self, addr: SocketAddr, cert_path: &str, key_path: &str) {
        tracing::info!("wsProxy listening on https://{} (TLS)", addr);

        let config = axum_server::tls_rustls::RustlsConfig::from_pem_file(cert_path, key_path)
            .await
            .unwrap();

        axum_server::tls_rustls::bind_rustls(addr, config)
            .serve(self.router.into_make_service())
            .await
            .unwrap();
    }
}

/// GET / — health check, returns "wsProxy running...\n" for compat.
async fn get_root() -> Html<String> {
    Html("wsProxy running...\n".to_string())
}

/// WebSocket upgrade handler for the `/ws` path used by clients that do not
/// encode the target address in the URL path (e.g., RagnarokRebuildTcp's
/// RebuildClient). The actual TCP target is resolved in priority order:
///
/// 1. The dedicated `--default-target` / `WSPROXY_DEFAULT_TARGET` value.
/// 2. A redirect entry with the special key `ws` (backward-compatible with the
///    original RebuildClient support).
/// 3. Reject with 400 if neither is configured.
async fn ws_upgrade_default(
    ws: axum::extract::ws::WebSocketUpgrade,
    state: axum::extract::State<Arc<AppState>>,
) -> axum::response::Response {
    let target = state
        .default_target
        .clone()
        .or_else(|| state.redirects.get("ws").cloned());

    let target = match target {
        Some(target) => target,
        None => {
            tracing::warn!("ws rejected: /ws requested but no default target configured");
            return (
                StatusCode::BAD_REQUEST,
                "no default target configured for /ws; use -d/--default-target or -r ws=<host>:<port>",
            )
                .into_response();
        }
    };

    match verify(&state, &target) {
        VerifyResult::Accepted(resolved_target) => {
            tracing::info!("ws upgrade: /ws -> target={}", resolved_target);
            ws.on_upgrade(move |socket| handle_socket(socket, resolved_target))
        }
        VerifyResult::Rejected(reason) => {
            tracing::warn!("ws rejected: {}", reason.0);
            (StatusCode::FORBIDDEN, reason.0).into_response()
        }
    }
}

/// WebSocket upgrade handler — extracts target from URL path, runs verify pipeline, then proxies.
async fn ws_upgrade(
    ws: axum::extract::ws::WebSocketUpgrade,
    state: axum::extract::State<Arc<AppState>>,
    target: axum::extract::Path<String>,
) -> axum::response::Response {
    let target: String = target.0;

    if !validate_target(&target) {
        tracing::warn!("ws rejected: invalid target format '{}'", target);
        // Reject at the HTTP layer so clients see a 4xx instead of a completed
        // WebSocket handshake that is immediately closed.
        return (StatusCode::BAD_REQUEST, "invalid target format").into_response();
    }

    match verify(&state, &target) {
        VerifyResult::Accepted(resolved_target) => {
            tracing::info!("ws upgrade: target={}", resolved_target);
            ws.on_upgrade(move |socket| handle_socket(socket, resolved_target))
        }
        VerifyResult::Rejected(reason) => {
            tracing::warn!("ws rejected: {}", reason.0);
            // Reject at the HTTP layer; do not complete the WebSocket upgrade.
            (StatusCode::FORBIDDEN, reason.0).into_response()
        }
    }
}
