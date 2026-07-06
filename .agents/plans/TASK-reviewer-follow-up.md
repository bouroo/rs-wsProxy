# Task: Address Gemini Code Assist review feedback

## Status

- Branch: `fix/reviewer-follow-up`
- PR: https://github.com/bouroo/rs-wsProxy/pull/9
- All unresolved review threads on PRs #1–#7 resolved and commented.
- @gemini-code-assist tagged for follow-up review.

## Fixes applied

| PR | File | Issue | Fix |
|----|------|-------|-----|
| #1 | `src/proxy.rs` | `SocketAddr::parse` fails for hostnames | Use `tokio::net::lookup_host(addr)` directly |
| #1 | `src/proxy.rs` | WS read errors silently ignored | Log and break on `Err(e)` |
| #1/#6 | `src/proxy.rs` | Per-read allocation / misleading comment | Use `BytesMut` + `split().freeze()` |
| #1 | `src/proxy.rs` | Redundant test code | Refactor `echo_server` helper |
| #5 | `src/proxy.rs` | Hardcoded unreachable port 1 | Use ephemeral closed port |
| #2 | `src/config.rs` + `src/modules.rs` | Fail-open on empty allow-list | `build_allowed_list` returns `Option<Vec<String>>`; `Some([])` = deny-all |
| #3 | `src/main.rs` + `src/server.rs` | `--threads 0` panic | Clamp to `1` |
| #3 | `src/main.rs` + `src/server.rs` | Abrupt shutdown | Graceful shutdown for plain (`with_graceful_shutdown`) and TLS (`axum_server::Handle`) |
| #4 | `src/config.rs` | `WSPROXY_SSL=false` parsed as true | Use `BoolishValueParser` |
| #4 | `src/config.rs` | CLI details in lib error messages | Return generic messages from `validate_tls_paths` |
| #4 | `src/main.rs` | Modules compiled twice | Import from `rs_ws_proxy` lib crate |
| #7 | `src/server.rs` | Invalid resolved target completes WS handshake | Validate `resolved_target` before `on_upgrade` |
| #7 | `src/server.rs` | Misleading route-order comment | Note Axum radix tree prioritizes static paths |

## Validation

- `cargo fmt --check` ✓
- `cargo clippy --all-targets -- -D warnings` ✓
- `cargo test` ✓ (44 unit + 10 integration + 13 server tests)

## Next action

Wait for CI on PR #9 and any further review from @gemini-code-assist.
