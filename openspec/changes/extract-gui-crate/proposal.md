## Why

The `esdiag` library is a single 76k-line crate, so every edit recompiles all of it: an incremental `cargo build` after any source edit takes about 17s. In the last six months, 38 of 269 source commits changed the server or templates without touching any library code below them, yet each of those edits recompiled all processing code. The desktop app is moving toward a wrapper around the same web interface, so the server and the Tauri wrapper belong together.

## What Changes

- Extract the web server and the Tauri desktop wrapper into a new `esdiag-gui` crate:
  - `src/server/` (including its static assets), `templates/`, and the `DocsAssets` and `ServerAssets` embeds.
  - The Tauri wrapper that starts the embedded server and opens it in a window, now in `main.rs`, as an `esdiag-gui` function the binary calls.
- Move the rest of the library into a new `esdiag-core` crate. Cargo rejects cyclic package dependencies, and the `esdiag` binary must depend on `esdiag-gui` while `esdiag-gui` depends on the library, so the library cannot stay in the `esdiag` package.
- Keep the root `esdiag` package as the CLI binary, Tauri context generation and bundle configuration, and a facade library that re-exports `esdiag-core` and `esdiag-gui` under today's module paths.
- Keep the `desktop` cargo feature, forwarded to `esdiag-gui`.
- Publish `esdiag-core` and `esdiag-gui` to crates.io with each `esdiag` release, in lockstep with its version, as implementation crates without a supported library API, so `cargo install esdiag` keeps working.
- Make both new packages self-contained for publication by bundling the repository files they embed (`assets/`, the ESDiag skill, `docs/`) before packaging.
- Add a build-timing script, record a baseline, and gate the change on measured incremental build times.
- No user-visible behavior changes to the CLI, Web UI, desktop app, setup assets, or processing output.

Non-goals:

- Splitting `esdiag-core` further (receiver, exporter, processor, setup). This is held until the timing results from this change are known.
- Supporting `esdiag-core` or `esdiag-gui` as libraries for use outside `esdiag`.

## Capabilities

### New Capabilities

- `workspace-crate-architecture`: The crate layout and dependency direction, lockstep publication with `esdiag`, self-contained packages, cargo feature preservation, rebuild scope, and build-timing gates.

### Modified Capabilities

None. Product behavior for Elasticsearch, Kibana, Logstash, and ECK diagnostics is unchanged.

## Impact

- Target products: all (Elasticsearch, Kibana, Logstash, ECK), as an internal restructuring with no change to processing output.
- Surfaces: Web UI and desktop code move to `esdiag-gui`; core processing and CLI code move to `esdiag-core`; none of their behavior changes.
- Code: `src/` except `main.rs`, `local.rs`, and `local/` moves to `crates/esdiag-core/src/`; `src/server/` and `templates/` move to `crates/esdiag-gui/`.
- Build: root `Cargo.toml` gains `[workspace.package]` and `[workspace.dependencies]`; `tauri.conf.json`, `desktop/`, and `cargo tauri build --features desktop` stay at the root.
- Release: each release publishes three packages (`esdiag-core`, `esdiag-gui`, `esdiag`) instead of one; the release skill updates. Multi-package publishing needs Cargo 1.90 or newer; the build MSRV stays 1.89.
- Tooling: new `bin/build-timings.sh` and `bin/sync-crate-assets.sh`; `crates/repo-tools` depends on `esdiag-core`; `.gitignore`, `AGENTS.md`, and `docs/repository/organization.md` update.
- Dependencies: no new third-party crates.
