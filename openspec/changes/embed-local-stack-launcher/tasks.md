## 1. Rust-Owned Local Lifecycle

- [x] 1.1 Remove rendered Bash launcher embedding and temporary Bash dispatch from the binary build and CLI.
- [x] 1.2 Add Rust `esdiag local` command parsing, structured outcomes, and binary-update guidance.
- [x] 1.3 Implement the shared stack-state schema, safe environment parsing, Compose generation, runtime detection, and state transitions in Rust.

## 2. Mode-Aware Local Stack Lifecycle

- [x] 2.1 Implement native `auto|full|core` lifecycle behavior: auto defaults to core and full remains an explicit container override.
- [x] 2.2 Implement full and core Compose lifecycle, image pulls, readiness, setup, browser controls, and reset in Rust.
- [x] 2.3 Implement managed native `esdiag serve` process ownership, logs, restart, and safe stop in Rust.
- [x] 2.4 Preserve standalone script self-update behavior and align its shared-state compatibility with the Rust lifecycle.
- [x] 2.5 Add Rust unit coverage for shared state, modes, and structured native output.

## 3. Initialization and Documentation

- [x] 3.1 Call the Rust core lifecycle from `esdiag init` after user approval and resume output setup.
- [x] 3.2 Update Agent Skill, CLI help, local-stack documentation, release/distribution guidance, and `CHANGELOG.md` for the Rust-owned binary path.
- [x] 3.3 Add initialization tests for accepted and declined core-stack startup without secret disclosure or arbitrary helper execution.
- [x] 3.4 Add cross-entry conformance tests for shared state, endpoints, secure credential handling, lifecycle controls, and mode-owned runtime state.

## 4. Full-Mode Container CLI

- [x] 4.1 Add `esdiag-local exec [launcher-options] -- <esdiag-arguments>` with full-mode state, dependency, and core-mode validation.
- [x] 4.2 Execute opaque child arguments through the state-derived Compose `esdiag` service, preserving terminal streams, exit status, image selection, network, and user-state volume.
- [x] 4.3 Add working-directory and explicit external-path mount support with safe path validation.
- [x] 4.4 Add managed-full-stack initialization support and separate internal Kibana API from browser-facing viewer URLs.
- [x] 4.5 Update the Agent Skill decision tree and command resolution to use `esdiag-local exec --` for container-only CLI workflows.
- [x] 4.6 Add standalone integration coverage for interactive passthrough, core-mode rejection, path mounts, internal/public URL separation, and no nested stack startup.

## 5. Verification

- [x] 5.1 Run `cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D warnings`, and `cargo test --all-features`.
- [x] 5.2 Run `tests/esdiag-local.sh` and relevant Rust/real-Compose validation for full and core modes.
- [x] 5.3 Run `openspec validate embed-local-stack-launcher --strict`.

## 6. Verification Follow-ups

- [x] 6.1 Preserve legacy no-mode state as full mode and protect credential/state coupling before native startup writes.
- [x] 6.2 Record and verify native-service process start identity, include native-service state in native structured lifecycle output, and preserve structured stdout from browser opening.
- [x] 6.3 Align native clipboard controls and lifecycle documentation with the standalone entry point and add state regression coverage.
