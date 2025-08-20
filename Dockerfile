# syntax=docker/dockerfile:1.4
# Use BuildKit syntax to enable cache mounts (speeds up cargo downloads / target caching)
# Requires BuildKit-enabled builder (GitHub Actions setup with docker/setup-buildx-action provides this)

# Multi-stage build with cargo-chef for optimal dependency caching
FROM rust:1.88 AS chef
RUN cargo install cargo-chef
WORKDIR /usr/src/app

FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS builder
# Copy the recipe and build dependencies first (this layer will be cached)
COPY --from=planner /usr/src/app/recipe.json recipe.json
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    --mount=type=cache,target=/usr/src/app/target \
    cargo chef cook --release --recipe-path recipe.json

# Copy source and build the application
COPY . .
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    --mount=type=cache,target=/usr/src/app/target \
    cargo build --release \
 && strip target/release/esdiag

FROM gcr.io/distroless/cc-debian12
COPY --from=builder /usr/src/app/target/release/esdiag /usr/local/bin/esdiag

ENTRYPOINT [ "/usr/local/bin/esdiag" ]
