# wsProxy — WebSocket to TCP Proxy

![CI](https://github.com/bouroo/rs-wsProxy/actions/workflows/ci.yml/badge.svg)
![CD](https://github.com/bouroo/rs-wsProxy/actions/workflows/cd.yml/badge.svg)
![License](https://img.shields.io/github/license/bouroo/rs-wsProxy)
![Release](https://img.shields.io/github/v/release/bouroo/rs-wsProxy)
![Docker Image](https://img.shields.io/badge/ghcr.io-bouroo%2Frs--wsProxy-blue)

A tiny, fast Rust bridge that lets WebSocket clients talk to plain TCP servers. It was built for [roBrowser](https://github.com/vthibault/roBrowser) and the RagnarokRebuildTcp client, but works with any TCP backend.

The target server is encoded directly in the WebSocket URL path:

```text
ws://proxy:5999/127.0.0.1:6900
```

## Why wsProxy?

- **Zero client changes for roBrowser**: the original WebSocket path convention is preserved.
- **Secure by default**: configure an allow-list so clients can only reach approved targets.
- **Flexible routing**: rewrite addresses on the fly with a redirect map.
- **TLS ready**: terminate WSS with rustls, or run behind a reverse proxy.
- **Container & Kubernetes friendly**: packaged as a Docker image and shipped with K8s manifests.

## Quick start

### Run from source

```bash
# Plain HTTP WebSocket on port 5999
cargo run

# With TLS, allow-list, and a redirect
cargo run -- \
  -p 8080 -s \
  -k ./default.key -c ./default.crt \
  -a "127.0.0.1:6900,127.0.0.1:5121" \
  -r "localhost:6900=login:6900"
```

### Run with Docker

```bash
docker run --rm -p 5999:5999 \
  ghcr.io/bouroo/rs-wsProxy:latest \
  -a "127.0.0.1:6900,127.0.0.1:5121"
```

### Run with Docker Compose

```bash
docker compose up --build
```

Edit `compose.yaml` to set the allow-list or other flags via `command` or environment variables.

### Run on Kubernetes

```bash
kubectl apply -f deploy/k8s/
kubectl port-forward svc/rs-wsProxy 5999:5999
```

## Client configuration

| Client | Server URL field |
|--------|-----------------|
| roBrowser | `ws://proxy:5999/127.0.0.1:6900` |
| RagnarokRebuildTcp RebuildClient | `ws://proxy:5999/ws` |

For RebuildClient, set the real TCP server on the proxy with `--default-target`:

```bash
cargo run -- -d "127.0.0.1:5000" -a "127.0.0.1:5000"
```

Or use the legacy redirect key `ws`:

```bash
cargo run -- -r "ws=127.0.0.1:5000" -a "127.0.0.1:5000"
```

## Configuration

Every flag has an equivalent `WSPROXY_*` environment variable.

| Flag | Description | Default | Environment Variable |
|------|-------------|---------|---------------------|
| `-p, --port` | Port to bind | `5999` | `WSPROXY_PORT` |
| `-t, --threads` | Tokio worker threads | `1` | `WSPROXY_THREADS` |
| `-s, --ssl` | Enable TLS | `false` | `WSPROXY_SSL` |
| `-k, --key` | SSL private key path | `./default.key` | `WSPROXY_KEY` |
| `-c, --cert` | SSL certificate path | `./default.crt` | `WSPROXY_CERT` |
| `-a, --allow` | Comma-separated allowed targets (`ip:port`) | empty (open proxy) | `WSPROXY_ALLOW` |
| `-r, --redirect` | Comma-separated redirects (`source=target`) | empty | `WSPROXY_REDIRECT` |
| `-d, --default-target` | Default target for RebuildClient `/ws` route | empty | `WSPROXY_DEFAULT_TARGET` |

## How it works

1. The client opens `ws://proxy:port/<target>`.
2. wsProxy validates the `host:port` format.
3. Any matching redirect is applied.
4. The resolved target is checked against the allow-list (empty list = open proxy).
5. The connection upgrades to WebSocket and a plain TCP socket is opened to the target.
6. Binary frames flow in both directions until one side closes.

## Security note

Running without an allow-list turns wsProxy into an open TCP relay. A warning is logged at startup. In production, always set `-a`/`WSPROXY_ALLOW` to the exact backends you want exposed.

## Deployment

### Container

Images are built and published to GitHub Container Registry automatically when a `v*` tag is pushed.

```bash
docker pull ghcr.io/bouroo/rs-wsProxy:latest
docker run -d --name wsproxy -p 5999:5999 \
  ghcr.io/bouroo/rs-wsProxy:latest \
  -a "10.0.0.10:6900,10.0.0.11:5121"
```

### Kubernetes

Manifests live in `deploy/k8s/`. To deploy a released version, edit the image tag in `deploy/k8s/deployment.yaml` or let the CD workflow do it for you.

Required secret for automatic K8s deployment:

- `KUBECONFIG` — base64-encoded kubeconfig for the target cluster.

The `deploy-k8s` job is skipped when the secret is absent, so container-only users are unaffected.

## Development

```bash
# Run tests
cargo test

# Lint
cargo fmt -- --check
cargo clippy --all-targets -- -D warnings

# Build release binary
cargo build --release
```

## Performance notes

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

## License

AGPL-3.0 — see [LICENSE](LICENSE).
