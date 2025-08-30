FROM cgr.dev/chainguard/rust:latest-dev AS builder

# Install build dependency for OpenSSL
USER root
RUN apk add --no-cache openssl-dev
USER nonroot

# Build Rust binary
COPY . .
RUN cargo build --release

# Wrap it in the wolfi-base container
FROM cgr.dev/chainguard/wolfi-base:latest
COPY --from=builder /work/target/release/esdiag /usr/bin/esdiag

ENTRYPOINT ["/usr/bin/esdiag"]
