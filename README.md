# wsProxy - WebSocket to TCP Proxy

A high-performance WebSocket-to-TCP proxy server written in Rust, compatible with roBrowser game client protocols.

## Features

- **WebSocket to TCP Proxy**: Forwards WebSocket binary frames to TCP endpoints
- **Allow List Security**: Restrict which target servers clients can connect to
- **Redirect Map**: Rewrite target addresses before proxying (e.g., `localhost:6900` → `login:6900`)
- **TLS Support**: Optional TLS via rustls for secure connections
- **Configurable**: Port, worker threads, SSL, allow list, and redirects via CLI args

## Usage

```bash
# Start with default settings (port 5999, no SSL)
cargo run

# With TLS and allow list
cargo run -- -p 8080 -s -k ./default.key -c ./default.crt -a "127.0.0.1:6900,127.0.0.1:5121" -r "localhost:6900=login:6900"
```

### CLI Options

| Flag | Description | Default |
|------|-------------|---------|
| `-p, --port` | Port to bind | `5999` |
| `-t, --threads` | Tokio worker threads | `1` |
| `-s, --ssl` | Enable TLS | `false` |
| `-k, --key` | SSL private key path | `./default.key` |
| `-c, --cert` | SSL certificate path | `./default.crt` |
| `-a, --allow` | Comma-separated allowed targets (ip:port) | empty (open proxy) |
| `-r, --redirect` | Comma-separated redirects (source=target) | empty |

## Architecture

```
Client (WebSocket) → wsProxy → Target Server (TCP)
                         │
                    ┌────┴─────┐
                    │ Verify   │
                    │ Pipeline │
                    └──────────┘
                         │
                    Redirect? → Allow List? → Accept/Reject
```

### Modules

- `config`: CLI args parsing (clap), AppState, allow list/redirect builders
- `logging`: Structured logging via tracing + env-filter
- `modules`: Verify pipeline (redirect → allow list check)
- `proxy`: TCP connect + bidirectional WebSocket↔TCP pump
- `server`: Axum HTTP/WebSocket server with TLS support

## Development Phases

### Phase 1: Core Proxy
- [x] WebSocket ↔ TCP bidirectional pump
- [x] Target validation (host:port format)
- [x] IPv4 DNS resolution

### Phase 2: Security & Routing
- [x] Allow list (target filtering)
- [x] Redirect map (address rewriting)
- [x] Verify pipeline (redirect → allow check)

### Phase 3: Server & TLS
- [x] Axum HTTP/WebSocket server
- [x] TLS support (rustls)
- [x] Health check endpoint

### Phase 4: CLI & Configuration
- [x] clap-based argument parsing
- [x] Environment variable config (RUST_LOG, WSPROXY_*)
- [x] Structured logging

### Phase 5: Testing & Validation
- [x] Unit tests for all modules
- [x] Integration tests (WebSocket + TCP echo)
- [x] Mutation testing config (cargo-mutants)

### Phase 6: Documentation & Polish
- [ ] README.md
- [ ] Code comments (why, not what)
- [ ] Performance profiling

## Testing

```bash
# Run all tests
cargo test

# Integration tests only
cargo test --test integration_test

# Mutation testing (if cargo-mutants available)
cargo mutants
```

## License

MIT License - see LICENSE file.
