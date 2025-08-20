# syntax=docker/dockerfile:1.4
# Use BuildKit syntax to enable cache mounts (speeds up cargo downloads / target caching)
# Requires BuildKit-enabled builder (GitHub Actions setup with docker/setup-buildx-action provides this)
FROM rust:1.88 AS builder
WORKDIR /usr/src/app

# Copy only manifest files first to cache dependency resolution layer
COPY Cargo.toml ./

# Create a tiny dummy source so cargo will populate the dependency graph and cache crates
RUN mkdir src && printf 'fn main() {}' > src/main.rs

# Populate cargo registry/git cache using BuildKit cache mounts
RUN --mount=type=cache,target=/usr/local/cargo/registry \
	--mount=type=cache,target=/usr/local/cargo/git \
	cargo build --release

# Remove dummy source
RUN rm -rf src

# Copy the full source and build, reusing cargo and target caches
COPY . .
RUN --mount=type=cache,target=/usr/local/cargo/registry \
	--mount=type=cache,target=/usr/local/cargo/git \
	--mount=type=cache,target=/usr/src/app/target \
	cargo build --release \
 && strip target/release/esdiag

FROM gcr.io/distroless/cc-debian12
COPY --from=builder /usr/src/app/target/release/esdiag /usr/local/bin/esdiag

ENTRYPOINT [ "/usr/local/bin/esdiag" ]
