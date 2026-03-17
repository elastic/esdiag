## 1. Host Config Model Updates

- [x] 1.1 Add `secret: Option<String>` and role fields to the host configuration model while preserving legacy auth fields.
- [x] 1.2 Update `hosts.yml` parsing/serialization to support default `collect` role assignment when roles are omitted.
- [x] 1.3 Implement validation rules enforcing `send` only for Elasticsearch hosts and `view` only for Kibana hosts.

## 2. Secret Store Foundation

- [x] 2.1 Evaluate and select a cross-platform Rust encryption approach compatible with self-contained distribution constraints.
- [x] 2.2 Define and implement the new encrypted secrets file format with versioning and `secret_id` lookup support.
- [x] 2.3 Implement keystore read/decrypt flow and map resolved secrets into runtime auth credentials.

## 3. Credential Resolution and Compatibility

- [x] 3.1 Implement credential precedence so `secret` references are used first, with fallback to legacy plaintext auth fields.
- [x] 3.2 Add validation errors for missing/unreadable `secret_id` references with clear host and secret context.
- [x] 3.3 Add migration-oriented docs/examples showing split `hosts.yml` + secrets file usage and optional legacy mode.

## 4. Role-Based Target Selection Integration

- [x] 4.1 Update collection/send/view target resolution to filter hosts by assigned roles.
- [x] 4.2 Ensure collection execution paths consume only role-constrained host lists for each workflow phase.
- [x] 4.3 Add integration coverage for mixed-role inventories across collect, send, and view workflows.

## 5. Verification

- [x] 5.1 Add/extend unit tests for host parsing, role defaults, role validation, and secret reference validation.
- [x] 5.2 Add/extend integration tests for secret-backed authentication and plaintext fallback behavior.
- [x] 5.3 Run `cargo clippy` and resolve any new warnings in changed code.
- [x] 5.4 Run `cargo test` and confirm all relevant suites pass.
