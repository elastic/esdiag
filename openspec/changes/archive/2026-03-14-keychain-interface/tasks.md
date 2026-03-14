## 1. Footer interaction refactor

- [x] 1.1 Update footer Datastar template/state so output selection supports saved host selection plus the live CLI-defined output target
- [x] 1.2 Keep host and secret management in the dedicated `/settings` UI rather than the footer flow
- [x] 1.3 Remove inline add-new-host controls from the output selector flow and adjust related UI actions

## 2. Session keychain unlock flow

- [x] 2.1 Implement backend keystore state machine with Datastar status fields `keystore.locked` and `keystore.lock_time` as backend-owned UI status
- [x] 2.2 Implement only idempotent `/keystore/unlock` and `/keystore/lock` endpoints (no toggle endpoint)
- [x] 2.3 Enforce HTTP semantics: invalid password on `/keystore/unlock` returns `401`; unavailable `/keystore/*` returns `404`
- [x] 2.4 Implement 12-hour user-mode session lease and refresh it on keystore reads and secure-host requests
- [x] 2.5 Add in-memory failed-unlock rate limiting (first 3 no delay, +5 minutes per failure from 4th, max 60 minutes)
- [x] 2.6 Add `Keystore` item to the user pop-up menu backed by explicit unlock/lock flows with lock confirmation
- [x] 2.7 Implement password field invalidation responses for incorrect unlock attempts
- [x] 2.8 Gate keystore routes/actions/UI with `cfg(feature = "keystore")` and runtime `service` mode route exclusion

## 3. Hosts and keychain manager modal

- [x] 3.1 Implement Datastar host/keychain management UI for `KnownHost` create/update/delete with full editable fields
- [x] 3.2 Implement backend handlers for host CRUD persistence in `hosts.yml` with validation and error responses
- [x] 3.3 Implement keychain metadata listing for modal auth selection using entry names only (no secret values in response)
- [x] 3.4 Implement host auth binding to selected keychain entry name and validate referenced entry existence on save
- [x] 3.5 Add locked/unlocked keystore status icon on manager edit page and keep it synchronized with backend session state
- [x] 3.6 Hide or disable manager keystore-specific controls when feature is disabled or runtime mode is `service`

## 4. Security and behavior hardening

- [x] 4.1 Enforce backend-only persisted secret handling so decrypted/ciphertext keychain values are never serialized to frontend state outside transient draft submission state
- [x] 4.2 Add guardrails to require unlock before keychain-backed reads/writes and return actionable locked-state errors
- [x] 4.3 Define secure host classification by auth type (`NoAuth` is non-secure; all other auth types are secure)
- [x] 4.4 Add secure-host processing preflight gate that blocks process start until keystore unlock succeeds
- [x] 4.5 On correct preflight password, unlock keystore and continue processing without requiring user to re-submit start action
- [x] 4.6 On incorrect preflight password, keep processing blocked and invalidate password field for retry
- [x] 4.7 Reject secure-host processing start with a keystore-unavailable error when feature is disabled or runtime mode is `service`
- [x] 4.8 Ensure secure-host processing lifecycle refreshes session lease on each keystore-backed host request
- [x] 4.9 Add missing-keystore bootstrap lifecycle handling in the web UI instead of auto-creating keystore storage at startup
- [x] 4.10 Add structured logging: INFO for successful auth and timeout closures, WARN for failed auth
- [x] 4.11 Ensure `settings.yml` active target updates remain consistent when host edits happen in the same session

## 5. Validation and regression coverage

- [x] 5.1 Add tests for idempotent `/keystore/unlock` and `/keystore/lock` behavior and PatchSignals updates for `keystore.*`
- [x] 5.2 Add tests for unlock HTTP semantics (`401` invalid password, `404` when keystore routes unavailable)
- [x] 5.3 Add tests for 12-hour lease initialization and lease refresh on keystore reads and secure-host processing requests
- [x] 5.4 Add tests for rate-limiting curve (3 free failures, +5 minutes per failure from 4th, max 60 minutes, reset on restart)
- [x] 5.5 Add integration/UI tests for `Keystore` user-menu flow, lock confirmation, and password field invalidation
- [x] 5.6 Add tests for secure-host classification by auth type (`NoAuth` bypass, others gated)
- [x] 5.7 Add tests for secure-host preflight unlock flow (success auto-continues, failure blocks and retries)
- [x] 5.8 Add tests for availability matrix: feature-enabled user mode, feature-disabled build, and `service` mode route absence
- [x] 5.9 Add startup tests for missing-keystore bootstrap lifecycle in user-mode web flows
- [x] 5.10 Add tests proving frontend payloads contain keychain names/metadata only and never persisted keychain secret values outside transient drafts
- [x] 5.11 Add regression coverage for footer changes (CLI output option, output selector restrictions, no inline host creation)
- [x] 5.12 Run `cargo clippy --all-targets --all-features`
- [x] 5.13 Run `cargo test`
