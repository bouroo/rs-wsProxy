# wsProxy - WebSocket to TCP Proxy

A high-performance WebSocket-to-TCP proxy server written in Rust, compatible with roBrowser game client protocols. The target TCP server is encoded in the WebSocket URL path, e.g. `ws://proxy:5999/127.0.0.1:6900`.

## Features

- **WebSocket to TCP Proxy**: Forwards WebSocket binary frames to TCP endpoints
- **Allow List Security**: Restrict which target servers clients can connect to
- **Redirect Map**: Rewrite target addresses before proxying (e.g., `localhost:6900` → `login:6900`)
- **TLS Support**: Optional TLS via rustls for secure connections
- **Configurable**: Port, worker threads, SSL, allow list, and redirects via CLI args or environment variables

## Usage

```bash
# Start with default settings (port 5999, no SSL)
cargo run

# With TLS and allow list
cargo run -- -p 8080 -s -k ./default.key -c ./default.crt -a "127.0.0.1:6900,127.0.0.1:5121" -r "localhost:6900=login:6900"

# Support both roBrowser and RagnarokRebuildTcp clients using the dedicated default target
cargo run -- -d "127.0.0.1:5000" -a "127.0.0.1:5000"

# Legacy fallback: configure the default target via the redirect map
cargo run -- -r "ws=127.0.0.1:5000" -a "127.0.0.1:5000"
```

### Client configuration examples

| Client | Server URL field |
|--------|-----------------|
| roBrowser | `ws://proxy:5999/127.0.0.1:6900` |
| RagnarokRebuildTcp RebuildClient | `ws://proxy:5999/ws` |

For the RebuildClient, the real TCP server is configured on the proxy side with `--default-target` (or `-r ws=<host>:<port>` as a legacy fallback).

### CLI Options

All options can also be set via `WSPROXY_*` environment variables (e.g., `WSPROXY_PORT=8080`).

| Flag | Description | Default | Environment Variable |
|------|-------------|---------|---------------------|
| `-p, --port` | Port to bind | `5999` | `WSPROXY_PORT` |
| `-t, --threads` | Tokio worker threads | `1` | `WSPROXY_THREADS` |
| `-s, --ssl` | Enable TLS | `false` | `WSPROXY_SSL` |
| `-k, --key` | SSL private key path | `./default.key` | `WSPROXY_KEY` |
| `-c, --cert` | SSL certificate path | `./default.crt` | `WSPROXY_CERT` |
| `-a, --allow` | Comma-separated allowed targets (ip:port) | empty (open proxy) | `WSPROXY_ALLOW` |
| `-r, --redirect` | Comma-separated redirects (source=target) | empty | `WSPROXY_REDIRECT` |
| `-d, --default-target` | Default target for RebuildClient `/ws` route | empty | `WSPROXY_DEFAULT_TARGET` |

## Protocol

Clients connect to `ws://proxy:port/<target>` where `<target>` is `host:port` of the TCP server. The proxy:

1. Validates the target format.
2. Applies any matching redirect.
3. Checks the allow-list (empty list = open proxy, logged at startup).
4. Upgrades to WebSocket and opens a TCP connection to the resolved target.
5. Pumps binary frames bidirectionally until either side closes.

### RebuildClient compatibility

The RagnarokRebuildTcp/RebuildClient does not encode the target address in the URL path. Instead it connects to a fixed `/ws` path. To support it from the same proxy, configure a default target:

```bash
cargo run -- -d "127.0.0.1:5000" -a "127.0.0.1:5000"
```

Then set RebuildClient's server field to `ws://proxy:5999/ws`.

If `--default-target` is not set, `/ws` falls back to a redirect entry with the special key `ws`:

```bash
cargo run -- -r "ws=127.0.0.1:5000" -a "127.0.0.1:5000"
```

**Requirement:** the backend must expose a plain TCP listener that accepts the same binary packets the RebuildClient sends inside its WebSocket frames. `rs-wsProxy` forwards the raw bytes; it does not translate WebSocket framing to a different TCP protocol.

## Performance

- TCP connections use `TCP_NODELAY` and IPv4 resolution to match the upstream Node.js behavior.
- The bidirectional pump splits the TCP stream into independent read/write halves, avoiding lock contention.
- Tune `-t/--threads` to match available CPU cores for high-throughput workloads.
- Use `RUST_LOG=debug` for request-level tracing; default level is `info`.

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
- [x] README.md
- [x] Code comments (why, not what)
- [x] Performance profiling note

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

AGPL-3.0 license - see LICENSE file.
