## Context

Currently, the `esdiag` binary embeds Elasticsearch and Kibana assets using the `include_dir!` macro. This creates uncompressed byte arrays in the binary, leading to significant bloat as more assets (mappings, dashboards, KB articles) are added. The goal is to replace this with a compressed bundle that is extracted at runtime during the `setup` phase.

## Goals / Non-Goals

**Goals:**
- Reduce binary size by compressing text-based assets.
- Automate the compression process within the Rust build pipeline.
- Maintain a single-binary distribution (no external asset files).
- Keep the `setup` command's behavior identical for the end-user.

**Non-Goals:**
- Compressing assets that are already compressed (e.g. existing .gz files).
- Implementing a generic filesystem abstraction for the entire app.
- On-the-fly decompression for frequent operations (only for `setup`).

## Decisions

- **Archive Format**: Use `.tar.gz` (Gzip compressed Tarball).
  - **Rationale**: Rust has excellent, mature support for `flate2` (Gzip) and `tar`. Gzip is highly effective for text-based assets (mappings, NDJSON). While Zip is also an option, Tar+Gzip is more idiomatic in many Rust contexts and fits the "stream of bytes" nature of embedded data well.
- **Build Integration**: Use a `build.rs` script to create the archive.
  - **Rationale**: `build.rs` allows us to run logic before the main compilation. It can scan the `assets/` directory, create the `.tar.gz` in `OUT_DIR`, and then the main code can use `include_bytes!` to pull it in.
- **Compression Library**: `flate2` with `zlib-ng` or `rust-backend`.
  - **Rationale**: `flate2` is the standard for Gzip in Rust.
- **Runtime Extraction**: Use the `tar` crate to stream entries from the embedded bytes.
  - **Rationale**: The `setup` phase already writes files to disk. We can swap the `include_dir` iteration with a `tar::Archive` iteration.

## Risks / Trade-offs

- **[Risk] Build Complexity** → Mitigation: Keep the `build.rs` simple and ensure it only runs when assets change using `cargo:rerun-if-changed`.
- **[Risk] Runtime Dependency** → Mitigation: Adding `flate2` and `tar` increases compile time slightly, but this is offset by the significantly smaller final binary.
- **[Risk] Decompression Overhead** → Mitigation: Decompression only happens during `setup`, which is a rare, one-time or upgrade-only operation. The cost is negligible (milliseconds).
