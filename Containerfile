# Build stage
FROM rust:1-trixie AS builder

WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY src ./src

RUN cargo build --release

# Runtime stage
FROM gcr.io/distroless/cc-debian13:nonroot

COPY --from=builder --chown=nonroot:nonroot /app/target/release/rs-wsProxy /usr/local/bin/rs-wsProxy

EXPOSE 5999

ENTRYPOINT ["/usr/local/bin/rs-wsProxy"]
CMD ["--help"]
