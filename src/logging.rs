use tracing_subscriber::EnvFilter;

/// Initialize structured logging from `RUST_LOG` (defaults to info).
pub fn init_logging() {
    let filter = EnvFilter::try_from_env("RUST_LOG").unwrap_or_else(|_| {
        EnvFilter::new("info")
    });

    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .try_init()
        .ok();
}

/// Log a loud warning that the proxy is running in open-proxy mode.
pub fn warn_open_proxy() {
    tracing::warn!(
        "⚠️  wsProxy is running in OPEN-PROXY mode — all targets allowed. \
         Restrict with --allow <ip:port>."
    );
}
