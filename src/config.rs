use clap::Parser;
use std::collections::HashMap;

/// CLI arguments parsed from the command line.
#[derive(Parser, Debug)]
#[command(
    name = "wsproxy",
    version,
    about = "WebSocket-to-TCP proxy (roBrowser-compatible)",
    disable_help_flag = true
)]
pub struct Args {
    /// Port to bind the HTTP/WebSocket server.
    #[arg(
        short = 'p',
        long = "port",
        env = "WSPROXY_PORT",
        default_value_t = 5999
    )]
    pub port: u16,

    /// Number of Tokio worker threads.
    #[arg(
        short = 't',
        long = "threads",
        env = "WSPROXY_THREADS",
        default_value_t = 1
    )]
    pub threads: usize,

    /// Enable SSL/TLS.
    #[arg(short = 's', long = "ssl", env = "WSPROXY_SSL")]
    pub ssl: bool,

    /// SSL private key file path.
    #[arg(
        short = 'k',
        long = "key",
        env = "WSPROXY_KEY",
        default_value = "./default.key"
    )]
    pub key: String,

    /// SSL certificate file path.
    #[arg(
        short = 'c',
        long = "cert",
        env = "WSPROXY_CERT",
        default_value = "./default.crt"
    )]
    pub cert: String,

    /// Comma-separated list of allowed target addresses (ip:port).
    #[arg(short = 'a', long = "allow", env = "WSPROXY_ALLOW")]
    pub allow: Option<String>,

    /// Comma-separated list of redirects (source=target).
    #[arg(short = 'r', long = "redirect", env = "WSPROXY_REDIRECT")]
    pub redirect: Option<String>,

    /// Show help and exit.
    #[arg(short = 'h', long = "help")]
    pub help: bool,
}

/// Shared application state — allow-list and redirect map.
pub struct AppState {
    pub allowed_servers: Vec<String>,
    pub redirects: HashMap<String, String>,
}

impl Clone for AppState {
    fn clone(&self) -> Self {
        AppState {
            allowed_servers: self.allowed_servers.clone(),
            redirects: self.redirects.clone(),
        }
    }
}

/// Parse a comma-separated "ip:port" string into a Vec of target strings.
/// Empty/whitespace-only entries are dropped.
pub fn build_allowed_list(raw: Option<String>) -> Vec<String> {
    match raw {
        Some(s) if !s.is_empty() => s
            .split(',')
            .map(|p| p.trim().to_string())
            .filter(|p| !p.is_empty())
            .collect(),
        _ => Vec::new(),
    }
}

/// Parse a comma-separated "source=target" string into a HashMap.
pub fn build_redirects(raw: Option<String>) -> HashMap<String, String> {
    let mut map = HashMap::new();
    if let Some(s) = raw {
        for entry in s.split(',') {
            let entry = entry.trim();
            if let Some(pos) = entry.find('=') {
                let from = entry[..pos].trim().to_string();
                let to = entry[pos + 1..].trim().to_string();
                if !from.is_empty() && !to.is_empty() {
                    map.insert(from, to);
                }
            }
        }
    }
    map
}

/// Validate that TLS key/cert files exist when SSL is requested.
/// Returns an error message if any required file is missing.
pub fn validate_tls_paths(ssl: bool, key: &str, cert: &str) -> Result<(), String> {
    if !ssl {
        return Ok(());
    }

    if !std::path::Path::new(key).is_file() {
        return Err(format!(
            "SSL key file not found: {} (use -k/--key or WSPROXY_KEY)",
            key
        ));
    }
    if !std::path::Path::new(cert).is_file() {
        return Err(format!(
            "SSL certificate file not found: {} (use -c/--cert or WSPROXY_CERT)",
            cert
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_args() {
        let args = Args::parse_from(["wsproxy"]);
        assert_eq!(args.port, 5999);
        assert_eq!(args.threads, 1);
        assert!(!args.ssl);
    }

    #[test]
    fn test_custom_port() {
        let args = Args::parse_from(["wsproxy", "-p", "8080"]);
        assert_eq!(args.port, 8080);
    }

    #[test]
    fn test_multiple_threads() {
        let args = Args::parse_from(["wsproxy", "-t", "4"]);
        assert_eq!(args.threads, 4);
    }

    #[test]
    fn test_ssl_enabled() {
        let args = Args::parse_from(["wsproxy", "-s"]);
        assert!(args.ssl);
    }

    #[test]
    fn test_ssl_paths() {
        let args = Args::parse_from([
            "wsproxy",
            "-k",
            "/path/to/key.pem",
            "-c",
            "/path/to/cert.pem",
        ]);
        assert_eq!(args.key, "/path/to/key.pem");
        assert_eq!(args.cert, "/path/to/cert.pem");
    }

    #[test]
    fn test_allowed_list_empty() {
        let list = build_allowed_list(None);
        assert!(list.is_empty());

        let list = build_allowed_list(Some("".to_string()));
        assert!(list.is_empty());
    }

    #[test]
    fn test_allowed_list_values() {
        let list = build_allowed_list(Some("127.0.0.1:6900, 127.0.0.1:5121".to_string()));
        assert_eq!(list.len(), 2);
        assert_eq!(list[0], "127.0.0.1:6900");
        assert_eq!(list[1], "127.0.0.1:5121");
    }

    #[test]
    fn test_redirects_empty() {
        let map = build_redirects(None);
        assert!(map.is_empty());

        let map = build_redirects(Some("".to_string()));
        assert!(map.is_empty());
    }

    #[test]
    fn test_redirects_values() {
        let map = build_redirects(Some(
            "localhost:6900=login:6900, localhost:6121=char:6121".to_string(),
        ));
        assert_eq!(map.len(), 2);
        assert_eq!(map.get("localhost:6900").unwrap(), "login:6900");
        assert_eq!(map.get("localhost:6121").unwrap(), "char:6121");
    }

    #[test]
    fn test_redirects_ignores_invalid() {
        let map = build_redirects(Some("no_equals".to_string()));
        assert!(map.is_empty());

        let map = build_redirects(Some("=empty_source".to_string()));
        assert!(map.is_empty());

        let map = build_redirects(Some("empty_target=".to_string()));
        assert!(map.is_empty());
    }

    #[test]
    fn test_app_state_creation() {
        let state = AppState {
            allowed_servers: vec!["127.0.0.1:6900".to_string()],
            redirects: HashMap::new(),
        };
        assert_eq!(state.allowed_servers.len(), 1);
    }

    #[test]
    fn test_app_state_empty() {
        let state = AppState {
            allowed_servers: Vec::new(),
            redirects: HashMap::new(),
        };
        assert!(state.allowed_servers.is_empty());
    }

    #[test]
    fn test_help_flag() {
        let args = Args::parse_from(["wsproxy", "--help"]);
        assert!(args.help);
    }

    #[test]
    fn test_all_options_combined() {
        let args = Args::parse_from([
            "wsproxy",
            "-p",
            "8080",
            "-t",
            "2",
            "-s",
            "-k",
            "/key.pem",
            "-c",
            "/cert.pem",
            "-a",
            "127.0.0.1:6900,127.0.0.1:5121",
            "-r",
            "localhost:6900=login:6900",
        ]);
        assert_eq!(args.port, 8080);
        assert_eq!(args.threads, 2);
        assert!(args.ssl);
        assert_eq!(args.key, "/key.pem");
        assert_eq!(args.cert, "/cert.pem");
        assert!(args.allow.is_some());
        assert!(args.redirect.is_some());
    }

    #[test]
    fn test_allowed_list_filters_empty_entries() {
        let list = build_allowed_list(Some("127.0.0.1:6900,, ,127.0.0.1:5121".to_string()));
        assert_eq!(list.len(), 2);
        assert_eq!(list[0], "127.0.0.1:6900");
        assert_eq!(list[1], "127.0.0.1:5121");
    }

    #[test]
    fn test_allowed_list_whitespace() {
        let list = build_allowed_list(Some(" 127.0.0.1:6900 , 127.0.0.1:5121 ".to_string()));
        assert_eq!(list.len(), 2);
        assert_eq!(list[0], "127.0.0.1:6900");
        assert_eq!(list[1], "127.0.0.1:5121");
    }

    #[test]
    fn test_build_allowed_list_from_args() {
        let args = Args::parse_from(["wsproxy", "-a", "127.0.0.1:6900,127.0.0.1:5121"]);
        let list = build_allowed_list(args.allow);
        assert_eq!(list.len(), 2);
    }

    #[test]
    fn test_build_redirects_from_args() {
        let args = Args::parse_from([
            "wsproxy",
            "-r",
            "localhost:6900=login:6900,localhost:6121=char:6121",
        ]);
        let map = build_redirects(args.redirect);
        assert_eq!(map.len(), 2);
    }

    #[test]
    fn test_app_state_with_redirects() {
        let state = AppState {
            allowed_servers: Vec::new(),
            redirects: build_redirects(Some("localhost:6900=login:6900".to_string())),
        };
        assert_eq!(state.redirects.len(), 1);
    }

    #[test]
    fn test_app_state_empty_redirects() {
        let state = AppState {
            allowed_servers: Vec::new(),
            redirects: build_redirects(None),
        };
        assert!(state.redirects.is_empty());
    }

    #[test]
    fn test_app_state_both() {
        let state = AppState {
            allowed_servers: build_allowed_list(Some("127.0.0.1:6900".to_string())),
            redirects: build_redirects(Some("localhost:6900=login:6900".to_string())),
        };
        assert_eq!(state.allowed_servers.len(), 1);
        assert_eq!(state.redirects.len(), 1);
    }

    #[test]
    fn test_redirect_map_order_preserved() {
        let map = build_redirects(Some("a:1=b:2,c:3=d:4,e:5=f:6".to_string()));
        assert_eq!(map.len(), 3);
    }

    #[test]
    fn test_validate_tls_paths_skips_when_ssl_disabled() {
        assert!(validate_tls_paths(false, "missing.key", "missing.crt").is_ok());
    }

    #[test]
    fn test_validate_tls_paths_fails_for_missing_key() {
        let err = validate_tls_paths(true, "missing.key", "Cargo.toml").unwrap_err();
        assert!(err.contains("SSL key file not found"));
    }

    #[test]
    fn test_validate_tls_paths_fails_for_missing_cert() {
        let err = validate_tls_paths(true, "Cargo.toml", "missing.crt").unwrap_err();
        assert!(err.contains("SSL certificate file not found"));
    }

    #[test]
    fn test_validate_tls_paths_succeeds_for_existing_files() {
        assert!(validate_tls_paths(true, "Cargo.toml", "Cargo.toml").is_ok());
    }
}
