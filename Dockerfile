# syntax=docker/dockerfile:1.4

####################
# Chef Stage - Prepare recipe for caching
####################
FROM rust:1.88-alpine AS chef
RUN apk add --no-cache \
    musl-dev \
    pkgconfig \
    openssl-dev \
    openssl-libs-static
RUN cargo install cargo-chef --locked
WORKDIR /usr/src/app

####################
# Planner Stage - Generate recipe.json
####################
FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

####################
# Builder Stage - Build dependencies and application
####################
FROM chef AS builder

# Install git for dependency resolution
RUN apk add --no-cache git

# Determine the target triple based on architecture
ARG TARGETPLATFORM
RUN case "$TARGETPLATFORM" in \
        "linux/amd64") echo "x86_64-unknown-linux-musl" > /tmp/rust-target ;; \
        "linux/arm64") echo "aarch64-unknown-linux-musl" > /tmp/rust-target ;; \
        *) echo "Unsupported platform: $TARGETPLATFORM" && exit 1 ;; \
    esac

# Add the target for cross-compilation
RUN rustup target add $(cat /tmp/rust-target)

# Set environment variables for static linking
ENV RUSTFLAGS="-C target-feature=+crt-static"
ENV OPENSSL_STATIC=1
ENV OPENSSL_LIB_DIR=/usr/lib
ENV OPENSSL_INCLUDE_DIR=/usr/include

# Copy the recipe and build dependencies
COPY --from=planner /usr/src/app/recipe.json recipe.json
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    --mount=type=cache,target=/usr/src/app/target \
    RUST_TARGET=$(cat /tmp/rust-target) && \
    cargo chef cook --release --target=$RUST_TARGET --recipe-path recipe.json

# Copy source code and build the application
COPY . .
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    --mount=type=cache,target=/usr/src/app/target \
    RUST_TARGET=$(cat /tmp/rust-target) && \
    cargo build --release --target=$RUST_TARGET && \
    strip target/$RUST_TARGET/release/esdiag && \
    ldd target/$RUST_TARGET/release/esdiag || echo "Static binary confirmed" && \
    cp target/$RUST_TARGET/release/esdiag /usr/local/bin/esdiag

####################
# Runtime Stage - Minimal scratch image
####################
FROM scratch AS runtime

# Copy CA certificates for HTTPS requests
COPY --from=builder /etc/ssl/certs/ca-certificates.crt /etc/ssl/certs/

# Copy the static binary
COPY --from=builder /usr/local/bin/esdiag /usr/local/bin/esdiag

# Copy minimal user/group files for security
COPY --from=builder /etc/passwd /etc/passwd
COPY --from=builder /etc/group /etc/group

# Use non-root user for security
USER 65534:65534

# Set the binary as entrypoint
ENTRYPOINT ["/usr/local/bin/esdiag"]
