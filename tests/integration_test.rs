use rs_ws_proxy::{AppState, Args};

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
fn test_allowed_list() {
    let args = Args::parse_from(["wsproxy", "-a", "127.0.0.1:6900,127.0.0.1:5121"]);
    assert!(args.allow.is_some());
}

#[test]
fn test_redirects() {
    let args = Args::parse_from([
        "wsproxy",
        "-r",
        "localhost:6900=login:6900,localhost:6121=char:6121",
    ]);
    assert!(args.redirect.is_some());
}

#[test]
fn test_app_state_creation() {
    let state = AppState {
        allowed_servers: vec!["127.0.0.1:6900".to_string()],
        redirects: std::collections::HashMap::new(),
    };

    assert_eq!(state.allowed_servers.len(), 1);
}

#[test]
fn test_build_allowed_list() {
    // Test empty string
    let list = super::build_allowed_list(None);
    assert!(list.is_empty());

    // Test with values
    let list = super::build_allowed_list(Some("127.0.0.1:6900,127.0.0.1:5121".to_string()));
    assert_eq!(list.len(), 2);
    assert_eq!(list[0], "127.0.0.1:6900");
    assert_eq!(list[1], "127.0.0.1:5121");
}

#[test]
fn test_build_redirects() {
    // Test empty string
    let map = super::build_redirects(None);
    assert!(map.is_empty());

    // Test with redirects
    let map = super::build_redirects(Some("localhost:6900=login:6900".to_string()));
    assert_eq!(map.len(), 1);
    assert_eq!(map.get("localhost:6900").unwrap(), "login:6900");
}
