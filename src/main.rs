use clap::{CommandFactory, Parser};
use std::net::SocketAddr;
use std::sync::Arc;

use rs_ws_proxy::config::{build_allowed_list, build_redirects, validate_tls_paths, Args};
use rs_ws_proxy::logging::warn_open_proxy;

fn main() {
    let args = Args::parse();

    // Show help and exit if requested.
    if args.help {
        println!("{}", Args::command().render_help());
        return;
    }

    // Initialize structured logging.
    rs_ws_proxy::logging::init_logging();

    // Build shared state.
    let state = Arc::new(rs_ws_proxy::config::AppState {
        allowed_servers: build_allowed_list(args.allow.clone()),
        redirects: build_redirects(args.redirect.clone()),
        default_target: args.default_target.clone(),
    });

    // Warn if running in open-proxy mode (allow-list was never configured).
    if state.allowed_servers.is_none() {
        warn_open_proxy();
    }

    let port = args.port;
    let addr: SocketAddr = format!("0.0.0.0:{port}").parse().unwrap();

    // A thread count of 0 would panic the Tokio runtime builder; clamp to 1.
    let threads = if args.threads == 0 {
        tracing::warn!("--threads 0 is invalid; using 1 worker thread");
        1
    } else {
        args.threads
    };

    if let Err(e) = validate_tls_paths(args.ssl, &args.key, &args.cert) {
        tracing::error!("{}", e);
        std::process::exit(1);
    }

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(threads)
        .enable_all()
        .build()
        .unwrap();

    runtime.block_on(async move {
        let shutdown = async {
            if let Err(e) = tokio::signal::ctrl_c().await {
                tracing::error!("failed to listen for ctrl-c: {}", e);
            }
            tracing::info!("shutdown signal received");
        };

        let server = rs_ws_proxy::server::Server::new(state);
        let result = if args.ssl {
            server
                .start_tls(addr, &args.cert, &args.key, shutdown)
                .await
        } else {
            server.start_plain(addr, shutdown).await
        };

        if let Err(e) = result {
            tracing::error!("Server error: {}", e);
            std::process::exit(1);
        }
    });
}
