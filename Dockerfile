FROM rust:1.86 AS builder
WORKDIR /usr/src/app
COPY . .
RUN cargo build --release

FROM gcr.io/distroless/cc-debian12
COPY --from=builder /usr/src/app/target/release/esdiag /usr/local/bin/esdiag

ENTRYPOINT [ "/usr/local/bin/esdiag" ]
