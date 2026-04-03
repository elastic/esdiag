## 1. Keystore Data Layer

- [ ] 1.1 Verify `delete_unlock_lease()` exists in `src/data/keystore.rs`; if not, add it alongside `write_unlock_lease()`
- [ ] 1.2 Confirm `read_unlock_lease()` is public and returns `Option<UnlockLease>` with expiry validation

## 2. ServerState Refactor

- [ ] 2.1 Remove `KeystoreSessionState` struct (fields: `locked`, `lock_time`, `expires_at_epoch`, `unlocked_password`) from `src/server/mod.rs`
- [ ] 2.2 Replace `keystore_state: Arc<RwLock<KeystoreSessionState>>` in `ServerState` with a minimal `keystore_rate_limit: Arc<Mutex<KeystoreRateLimit>>` struct holding only `failed_attempts` and backoff metadata
- [ ] 2.3 Update all `ServerState` construction sites to use the new field

## 3. Route Handler Updates

- [ ] 3.1 Update `/keystore/unlock` handler in `src/server/keystore.rs` to call `write_unlock_lease(password, 24h)` on success instead of updating in-memory state
- [ ] 3.2 Update `/keystore/lock` handler to call `delete_unlock_lease()` instead of clearing in-memory state
- [ ] 3.3 Update keystore status signal emission to read `read_unlock_lease()` and derive `keystore.locked` / `keystore.lock_time` from the file result
- [ ] 3.4 Remove any code paths that read `KeystoreSessionState.unlocked_password` for credential resolution in web request handlers

## 4. Bootstrap Flow

- [ ] 4.1 After the web bootstrap modal creates a new keystore, call `write_unlock_lease(password, 24h)` so the process immediately reflects unlocked state

## 5. Keystore Preflight and Credential Resolution

- [ ] 5.1 Update `web-secure-processing-gate` preflight check to derive lock state from `read_unlock_lease()` rather than in-memory session state
- [ ] 5.2 Confirm credential resolution for web-initiated processing reads the unlock file (via `host-secret-store` resolution chain) — no in-memory password injection

## 6. `/keystore/status` Endpoint

- [ ] 6.1 Add `GET /keystore/status` route to `src/server/keystore.rs` that reads `read_unlock_lease()` and returns JSON with `locked: bool` and `expires_at_epoch: Option<i64>`
- [ ] 6.2 Mount the route alongside `/keystore/unlock` and `/keystore/lock` (user mode only, returns 404 in service mode)

## 7. Integration Tests (`tests/keystore_web_unlock_tests.rs`)

- [ ] 7.1 Add test `web_unlock_writes_unlock_file`: start server in user mode with a pre-created keystore in a `TempDir`; POST `/keystore/unlock` with the correct password; assert `keystore.unlock` exists on disk; assert `GET /keystore/status` returns `locked: false`
- [ ] 7.2 Add test `web_lock_deletes_unlock_file`: follow up 7.1 with POST `/keystore/lock`; assert `keystore.unlock` no longer exists; assert `GET /keystore/status` returns `locked: true`
- [ ] 7.3 Add test `cli_unlock_reflected_in_web_status`: write a valid unlock lease via `write_unlock_lease()` directly (no CLI subprocess); assert `GET /keystore/status` returns `locked: false` without any web unlock call
- [ ] 7.4 Add test `cli_lock_reflected_in_web_status`: after 7.3, delete the unlock file via `run_esdiag(&["keystore", "lock"], ...)`; assert `GET /keystore/status` returns `locked: true`
- [ ] 7.5 Add test `web_unlock_status_verified_by_cli`: POST `/keystore/unlock` via HTTP; call `get_unlock_status()` (library helper) directly; assert `unlock_active == true` and `expires_at_epoch` is set

## 8. Delta Spec Archive

- [ ] 8.1 Run `openspec verify --change unify-unlock` to confirm all spec changes are coherent
- [ ] 8.2 Ensure the `web-keychain-session-unlock` delta spec is complete before archiving

## 9. Verification

- [ ] 9.1 Run `cargo clippy` and resolve all warnings
- [ ] 9.2 Run `cargo test` and ensure all keystore tests pass (both existing and new)
