use clap::{CommandFactory, Parser};
use std::net::SocketAddr;
use std::sync::Arc;

mod config;
mod logging;
mod modules;
mod proxy;
mod server;

use config::{build_allowed_list, build_redirects, Args};
use logging::warn_open_proxy;

fn main() {
    let args = Args::parse();

    // Show help and exit if requested.
    if args.help {
        println!("{}", Args::command().render_help());
        return;
    }

    // Initialize structured logging.
    logging::init_logging();

    // Build shared state.
    let state = Arc::new(config::AppState {
        allowed_servers: build_allowed_list(args.allow.clone()),
        redirects: build_redirects(args.redirect.clone()),
    });

    // Warn if running in open-proxy mode.
    if state.allowed_servers.is_empty() {
        warn_open_proxy();
    }

    let port = args.port;
    let addr: SocketAddr = format!("0.0.0.0:{port}").parse().unwrap();

    let server = server::Server::new(state);

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(args.threads)
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

        tokio::select! {
            _ = shutdown => {}
            _ = run_server(server, addr, args.ssl, args.key, args.cert) => {}
        }
    });
}

async fn run_server(
    server: server::Server,
    addr: SocketAddr,
    ssl: bool,
    key: String,
    cert: String,
) {
    if ssl {
        server.start_tls(addr, &key, &cert).await;
    } else {
        server.start_plain(addr).await;
    }
}
