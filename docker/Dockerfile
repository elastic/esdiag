FROM cgr.dev/chainguard/rust:latest-dev AS builder

# Link OpenSSL 3 so binaries extracted from the image run on common Linux hosts
USER root
RUN apk add --no-cache openssl-3.6-dev
USER nonroot

# Build Rust binary
COPY . .
RUN ESDIAG_GENERATE_NOTICE=0 cargo build --release

# Wrap it in the wolfi-base container
FROM cgr.dev/chainguard/wolfi-base:latest
RUN apk add --no-cache openssl-3.6
COPY --from=builder /work/target/release/esdiag /usr/bin/esdiag

# Published container ports cannot reach a server bound to loopback.
ENV ESDIAG_BIND=0.0.0.0

ENTRYPOINT ["/usr/bin/esdiag"]
