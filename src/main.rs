use clap::Parser;
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

    if args.ssl {
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(async {
                server.start_tls(addr, &args.key, &args.cert).await;
            });
    } else {
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(async {
                server.start_plain(addr).await;
            });
    }
}
