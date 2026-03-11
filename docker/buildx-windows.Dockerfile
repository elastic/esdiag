# syntax=docker/dockerfile:1.7

FROM cgr.dev/chainguard/rust:latest-dev AS builder

USER root
RUN apk add --no-cache \
    bash \
    llvm \
    pkgconf \
    zig
USER nonroot

RUN rustup toolchain install stable \
    && rustup default stable

RUN TOOLCHAIN="$(rustup show active-toolchain | awk '{print $1}')" \
    && rustup target add --toolchain "${TOOLCHAIN}" x86_64-pc-windows-gnullvm \
    && printf '%s' "${TOOLCHAIN}" > /tmp/rust-toolchain-name

RUN mkdir -p "${HOME}/.local/bin" \
    && printf '%s\n' '#!/bin/sh' 'exec /usr/bin/llvm-windres --no-preprocess "$@"' > "${HOME}/.local/bin/x86_64-w64-mingw32-windres" \
    && chmod +x "${HOME}/.local/bin/x86_64-w64-mingw32-windres"

RUN TOOLCHAIN="$(cat /tmp/rust-toolchain-name)" \
    && TOOLCHAIN_BIN="${HOME}/.rustup/toolchains/${TOOLCHAIN}/bin" \
    && PATH="${HOME}/.local/bin:${TOOLCHAIN_BIN}:${HOME}/.cargo/bin:${PATH}" \
       cargo install --locked cargo-zigbuild

ENV ESDIAG_GENERATE_NOTICE=0
ENV CFLAGS_x86_64_pc_windows_gnullvm="-Wno-error=date-time -Wno-unknown-pragmas"

WORKDIR /work
COPY --chown=nonroot:nonroot . .

RUN TOOLCHAIN="$(cat /tmp/rust-toolchain-name)" \
    && TOOLCHAIN_BIN="${HOME}/.rustup/toolchains/${TOOLCHAIN}/bin" \
    && PATH="${HOME}/.local/bin:${TOOLCHAIN_BIN}:${HOME}/.cargo/bin:${PATH}" \
       cargo zigbuild --release --features desktop --target x86_64-pc-windows-gnullvm

RUN mkdir -p /work/out \
    && cp target/x86_64-pc-windows-gnullvm/release/esdiag.exe /work/out/ \
    && cp target/x86_64-pc-windows-gnullvm/release/WebView2Loader.dll /work/out/

FROM scratch AS artifacts
COPY --from=builder /work/out/ /
