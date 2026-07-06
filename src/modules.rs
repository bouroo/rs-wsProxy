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
///   This matches the upstream Node.js module order and lets operators alias
///   internal hostnames while still enforcing the allow-list on the final target.
/// - An empty allow list means "allow all" (open proxy).
pub fn verify(state: &AppState, target: &str) -> VerifyResult {
    // 1. Redirect — rewrite the URL if a matching entry exists.
    let target = match state.redirects.get(target) {
        Some(redirected) => redirected.clone(),
        None => target.to_string(),
    };

    // 2. Allow — `None` = allow all (open proxy); `Some(list)` = enforce it.
    //    An explicit empty list means "deny all".
    match state.allowed_servers.as_ref() {
        None => VerifyResult::Accepted(target),
        Some(list) if list.is_empty() => VerifyResult::Rejected(RejectReason(
            "allow-list is configured but empty — denying all targets".to_string(),
        )),
        Some(list) => {
            if list.contains(&target) {
                VerifyResult::Accepted(target)
            } else {
                VerifyResult::Rejected(RejectReason(format!(
                    "target '{}' not in allow list",
                    target
                )))
            }
        }
    }
}

/// Validate that `target` looks like a valid host:port.
///
/// The last colon separates the host from the port; the host must be non-empty
/// and the port must parse as a valid `u16`. This rejects malformed targets
/// such as `127.0.0.1:abc` or `127.0.0.1:` before the WebSocket handshake.
pub fn validate_target(target: &str) -> bool {
    if let Some(pos) = target.rfind(':') {
        let host = &target[..pos];
        let port = &target[pos + 1..];
        if host.is_empty() || port.parse::<u16>().is_err() {
            return false;
        }
        // IPv6 addresses contain colons; with a port they must be bracketed as
        // [ipv6]:port. Reject bare IPv6 strings like ::1 or 2001:db8::1 that
        // could otherwise be mistaken for host:port pairs.
        if host.contains(':') && (!host.starts_with('[') || !host.ends_with(']')) {
            return false;
        }
        true
    } else {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn empty_state() -> AppState {
        AppState {
            allowed_servers: None,
            redirects: HashMap::new(),
            default_target: None,
        }
    }

    #[test]
    fn test_verify_empty_allow_all() {
        let state = empty_state();
        assert!(matches!(
            verify(&state, "127.0.0.1:6900"),
            VerifyResult::Accepted(_)
        ));
    }

    #[test]
    fn test_verify_allow_list_hit() {
        let mut state = empty_state();
        state.allowed_servers = Some(vec!["127.0.0.1:6900".to_string()]);
        assert!(matches!(
            verify(&state, "127.0.0.1:6900"),
            VerifyResult::Accepted(_)
        ));
    }

    #[test]
    fn test_verify_allow_list_miss() {
        let mut state = empty_state();
        state.allowed_servers = Some(vec!["127.0.0.1:6900".to_string()]);
        match verify(&state, "127.0.0.1:9999") {
            VerifyResult::Rejected(reason) => {
                assert_eq!(reason.0, "target '127.0.0.1:9999' not in allow list")
            }
            _ => panic!("expected rejection"),
        }
    }

    #[test]
    fn test_verify_empty_allow_list_denies_all() {
        // A configured but empty allow-list must be treated as "deny all", not
        // "allow all".
        let mut state = empty_state();
        state.allowed_servers = Some(Vec::new());
        match verify(&state, "127.0.0.1:6900") {
            VerifyResult::Rejected(_) => {}
            _ => panic!("expected rejection for empty configured allow-list"),
        }
    }

    #[test]
    fn test_verify_redirect_rewrites_then_allows() {
        let mut state = empty_state();
        state.allowed_servers = Some(vec!["login:6900".to_string()]);

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
        assert!(!validate_target("127.0.0.1:"));
        assert!(!validate_target("127.0.0.1:abc"));
        assert!(!validate_target(":6900"));
        assert!(!validate_target("127.0.0.1:70000"));
        // Bare IPv6 addresses must not be accepted as host:port pairs.
        assert!(!validate_target("::1"));
        assert!(!validate_target("2001:db8::1"));
    }

    #[test]
    fn test_validate_target_ipv6_bracketed() {
        assert!(validate_target("[::1]:6900"));
        assert!(validate_target("[2001:db8::1]:5121"));
    }

    #[test]
    fn test_verify_order_redirect_then_allow() {
        // Redirect "a:1" → "b:2", allow list has only "b:2".
        let mut state = empty_state();
        state.allowed_servers = Some(vec!["b:2".to_string()]);

        let mut redirects = HashMap::new();
        redirects.insert("a:1".to_string(), "b:2".to_string());
        state.redirects = redirects;

        // Direct allow-list check of "a:1" would fail, but redirect resolves it.
        match verify(&state, "a:1") {
            VerifyResult::Accepted(t) => assert_eq!(t, "b:2"),
            _ => panic!("expected accept after redirect resolution"),
        }
    }

    #[test]
    fn test_verify_redirect_does_not_chain() {
        // Only the first redirect is applied; no recursive resolution.
        let mut state = empty_state();
        let mut redirects = HashMap::new();
        redirects.insert("a:1".to_string(), "b:2".to_string());
        redirects.insert("b:2".to_string(), "c:3".to_string());
        state.redirects = redirects;

        match verify(&state, "a:1") {
            VerifyResult::Accepted(t) => assert_eq!(t, "b:2"),
            _ => panic!("expected single-step redirect"),
        }
    }

    #[test]
    fn test_verify_empty_target_rejected_when_allow_list_present() {
        let mut state = empty_state();
        state.allowed_servers = Some(vec!["127.0.0.1:6900".to_string()]);

        match verify(&state, "") {
            VerifyResult::Rejected(_) => {}
            _ => panic!("expected rejection for empty target"),
        }
    }
}
