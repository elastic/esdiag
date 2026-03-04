## 1. Runtime Mode Foundations

- [x] 1.1 Add a web-facing `RuntimeMode` model (`service`/`user`) and thread it through `serve` and desktop web bootstrap paths only.
- [x] 1.2 Implement runtime mode resolution precedence (`--mode` > `ESDIAG_MODE` > default) and add startup validation for invalid combinations.
- [x] 1.3 Ensure CLI command paths bypass runtime mode plumbing and preserve current behavior.
- [x] 1.4 Add startup logging that prints `Starting ${mode}-mode server on port ${port_number}` and emits mode diagnostics at `LOG_LEVEL=debug` or higher.

## 2. Mode Policy and Server State

- [x] 2.1 Introduce a mode policy boundary (for example `RuntimeModePolicy`) for auth source, persistence permissions, preference scope, and exporter mutability.
- [x] 2.2 Refactor web server state/services to call mode policy instead of direct `hosts.yml`/settings/auth decisions in handlers.
- [x] 2.3 Add tests for mode policy decisions covering service and user behavior matrices.

## 3. Authentication and Persistence Controls

- [x] 3.1 Implement `service` mode request identity extraction from required identity-aware-proxy headers and reject missing/invalid headers.
- [x] 3.2 Implement `service` mode guards that skip reads/writes to `hosts.yml` and related local artifacts.
- [x] 3.3 Keep `user` mode local credential and `hosts.yml` read/write behavior available, with optional secret handling hooks for credential encryption.

## 4. Settings and Exporter UX Behavior

- [x] 4.1 Update settings endpoints/UI state models to expose limited preferences in `service` mode and full preferences in `user` mode.
- [x] 4.2 Enforce fixed startup exporter in `service` mode and runtime exporter configurability in `user` mode.
- [x] 4.3 Add/adjust UI integration tests for mode-specific settings and exporter interaction behavior in `serve` and desktop-hosted web contexts.

## 5. Validation and Regression Coverage

- [x] 5.1 Add integration tests proving runtime mode behavior applies only to web interfaces and not to CLI execution.
- [x] 5.2 Run `cargo clippy` and resolve any new warnings introduced by runtime mode changes.
- [x] 5.3 Run `cargo test` and confirm mode-specific web tests plus existing CLI tests pass.
