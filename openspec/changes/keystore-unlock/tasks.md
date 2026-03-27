## 1. CLI command surface

- [x] 1.1 Extend `KeystoreCommands` in `src/main.rs` with `unlock`, `lock`, `status`, `password`, and `update`, including `--ttl` parsing for `unlock`.
- [x] 1.2 Update CLI handlers so `keystore add` is create-only, `keystore update` is exist-only, `keystore password` performs password rotation, and interactive `add`/`update` flows prompt with masked input when required API key or password values are missing.
- [x] 1.3 Update CLI help text and user-facing logging for unlock leases, TTL validation, bootstrap prompts, and status output.

## 2. Keystore data-layer behavior

- [x] 2.1 Refactor `src/data/keystore.rs` to separate keystore creation, existing-password validation, secret creation, secret update, and password rotation helpers.
- [x] 2.2 Add unlock lease read/write helpers for `~/.esdiag/keystore.unlock`, including versioned envelope handling, minimal encryption, expiration checks, best-effort deletion, and restrictive file permissions where supported.
- [x] 2.3 Implement TTL parsing and validation with accepted suffixes `m`, `h`, and `d`, a default of 24 hours, and a maximum of 30 days.

## 3. Host secret resolution and bootstrap flows

- [x] 3.1 Extend keystore password lookup precedence to check scoped password first, `ESDIAG_KEYSTORE_PASSWORD` second, and a valid unexpired unlock lease third.
- [x] 3.2 Implement interactive `keystore unlock` bootstrap behavior for missing keystores and non-interactive refusal behavior without implicit keystore creation.
- [x] 3.3 Ensure expired, malformed, or undeletable unlock files are treated as locked state without breaking explicit environment-based or interactive fallback flows.
- [x] 3.4 Allow user-mode web sessions to seed their initial in-memory unlock state from an existing valid CLI unlock lease without creating a new lease or refreshing/extending a valid lease; readers may still perform best-effort deletion of expired or stale lease files when reading them.

## 4. Verification and documentation

- [x] 4.1 Add or update tests for unlock, lock, status, TTL parsing, lease expiry cleanup, bootstrap behavior, add/update semantics, and password rotation.
- [x] 4.2 Update user documentation in `readme.md`, `docs/hosts-keystore.md`, and any CLI reference content to describe unlock leases, the new command set, and masked prompting for secret entry in interactive shells.
- [x] 4.3 Run `cargo clippy` and `cargo test` and address any failures introduced by the change.
