use std::collections::HashMap;

use crate::config::AppState;

/// Reason the verify pipeline rejected a connection.
#[derive(Debug, Clone)]
pub struct RejectReason(pub String);

/// Result of running the verify pipeline.
#[derive(Debug)]
pub enum VerifyResult {
    /// Connection accepted with the resolved target.
    Accepted(String),
    /// Connection rejected — close WS upgrade.
    Rejected(RejectReason),
}

/// Run the verify pipeline (redirect → allow) against `state`.
///
/// - Redirects run first so the allow-list check sees the *rewritten* target.
/// - An empty allow list means "allow all" (open proxy).
pub fn verify(state: &AppState, target: &str) -> VerifyResult {
    // 1. Redirect — rewrite the URL if a matching entry exists.
    let target = match state.redirects.get(target) {
        Some(redirected) => redirected.clone(),
        None => target.to_string(),
    };

    // 2. Allow — empty list = allow all (with warning).
    if state.allowed_servers.is_empty() {
        return VerifyResult::Accepted(target);
    }

    if state.allowed_servers.contains(&target) {
        VerifyResult::Accepted(target)
    } else {
        VerifyResult::Rejected(RejectReason(format!(
            "target '{}' not in allow list",
            target
        )))
    }
}

/// Validate that `target` looks like a valid host:port.
pub fn validate_target(target: &str) -> bool {
    target.contains(':') && !target.is_empty()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn empty_state() -> AppState {
        AppState {
            allowed_servers: Vec::new(),
            redirects: HashMap::new(),
        }
    }

    #[test]
    fn test_verify_empty_allow_all() {
        let state = empty_state();
        assert!(matches!(verify(&state, "127.0.0.1:6900"), VerifyResult::Accepted(_)));
    }

    #[test]
    fn test_verify_allow_list_hit() {
        let mut state = empty_state();
        state.allowed_servers.push("127.0.0.1:6900".to_string());
        assert!(matches!(verify(&state, "127.0.0.1:6900"), VerifyResult::Accepted(_)));
    }

    #[test]
    fn test_verify_allow_list_miss() {
        let mut state = empty_state();
        state.allowed_servers.push("127.0.0.1:6900".to_string());
        match verify(&state, "127.0.0.1:9999") {
            VerifyResult::Rejected(reason) => assert_eq!(reason.0, "target '127.0.0.1:9999' not in allow list"),
            _ => panic!("expected rejection"),
        }
    }

    #[test]
    fn test_verify_redirect_rewrites_then_allows() {
        let mut state = empty_state();
        state.allowed_servers.push("login:6900".to_string());

        let mut redirects = HashMap::new();
        redirects.insert("localhost:6900".to_string(), "login:6900".to_string());
        state.redirects = redirects;

        // "localhost:6900" → redirect to "login:6900" → matches allow list.
        match verify(&state, "localhost:6900") {
            VerifyResult::Accepted(t) => assert_eq!(t, "login:6900"),
            _ => panic!("expected accept after redirect"),
        }
    }

    #[test]
    fn test_verify_redirect_no_match() {
        let state = empty_state();
        match verify(&state, "unknown:1234") {
            VerifyResult::Accepted(t) => assert_eq!(t, "unknown:1234"),
            _ => panic!("expected accept (no allow list)"),
        }
    }

    #[test]
    fn test_validate_target_valid() {
        assert!(validate_target("127.0.0.1:6900"));
        assert!(validate_target("localhost:5121"));
    }

    #[test]
    fn test_validate_target_invalid() {
        assert!(!validate_target(""));
        assert!(!validate_target("no_colon"));
    }

    #[test]
    fn test_verify_order_redirect_then_allow() {
        // Redirect "a:1" → "b:2", allow list has only "b:2".
        let mut state = empty_state();
        state.allowed_servers.push("b:2".to_string());

        let mut redirects = HashMap::new();
        redirects.insert("a:1".to_string(), "b:2".to_string());
        state.redirects = redirects;

        // Direct allow-list check of "a:1" would fail, but redirect resolves it.
        match verify(&state, "a:1") {
            VerifyResult::Accepted(t) => assert_eq!(t, "b:2"),
            _ => panic!("expected accept after redirect resolution"),
        }
    }
}
