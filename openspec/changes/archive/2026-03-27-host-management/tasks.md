## 1. CLI Update Mode

- [x] 1.1 Add a host-update override model that records which `esdiag host` fields were explicitly supplied, including `--accept-invalid-certs true|false` certificate validation overrides.
- [x] 1.2 Update `Commands::Host` parsing and command dispatch so full-definition, delete, incremental-update, and validation-only host invocations are distinguished reliably.
- [x] 1.3 Add `--delete` as a mutually exclusive host-management option for removing an existing saved host.

## 2. Saved Host Merge Flow

- [x] 2.1 Implement merge helpers for existing `KnownHost` records so auth, roles, and certificate validation overrides can be applied without restating `app` and `url`.
- [x] 2.2 Keep explicit create-or-replace behavior when `app` and `url` are supplied, and return clear errors for incremental updates against missing host names.
- [x] 2.3 Reuse host normalization, validation, and live connection testing before persistence for every incremental update so only successful merged records are saved.
- [x] 2.4 Implement saved-host deletion persistence and any dependent local settings cleanup for `esdiag host <name> --delete`.

## 3. Verification

- [x] 3.1 Add CLI regression tests for secret, API key, role, and certificate update flows, including omitted, `true`, and `false` `--accept-invalid-certs` behavior, delete success, delete conflicts, and missing-host failures.
- [x] 3.2 Run `cargo clippy`.
- [x] 3.3 Run `cargo test`.
