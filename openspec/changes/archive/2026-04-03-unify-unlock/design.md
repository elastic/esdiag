## Context

The web server currently holds unlock state in `KeystoreSessionState` — an in-memory struct stored as `Arc<RwLock<KeystoreSessionState>>` inside `ServerState`. When a user unlocks via the web UI, this struct records the decrypted password, lock status, and a 12-hour expiry. This state is invisible to the CLI and to the Agent Skill, which reads only the `keystore.unlock` file on disk.

The CLI uses `write_unlock_lease()` / `read_unlock_lease()` in `src/data/keystore.rs`. These functions read and write `keystore.unlock` (an `EncryptedUnlockLease` stored as YAML) alongside the active keystore path. The credential resolution chain in `host-secret-store` already reads this file as a fallback, so any process that writes it correctly becomes a first-class unlock source.

## Goals / Non-Goals

**Goals:**
- Web `/keystore/unlock` writes `keystore.unlock` on success, making unlock visible to the CLI, Agent Skill, and any other process
- Web `/keystore/lock` deletes `keystore.unlock`, same as `esdiag keystore lock`
- Lock state queries read the file rather than in-memory struct
- Rate limiting (failed attempt backoff) remains in-memory only

**Non-Goals:**
- Changing the unlock file format, TTL defaults, or encryption scheme
- Modifying the CLI unlock commands
- Adding cross-process file-watch notifications (polling or on-demand reads are sufficient)
- Changing service mode behavior (keystore routes remain unmounted)

## Decisions

**Decision: Remove `KeystoreSessionState` entirely, keep only `failed_attempts` in `ServerState`**

The session struct currently holds `locked`, `lock_time`, `expires_at_epoch`, and `unlocked_password` in addition to `failed_attempts`. After this change, lock state is derived by reading the unlock file. Only `failed_attempts` (and associated backoff metadata) remains in `ServerState` because rate limiting is intentionally non-persistent.

Alternative considered: keep `KeystoreSessionState` and treat the file as a cache. Rejected — dual state sources create split-brain risk and add complexity with no benefit.

**Decision: Read the unlock file on every lock-state check, no in-memory caching**

Each status poll, keystore preflight, and route handler that needs to know whether the keystore is unlocked reads `read_unlock_lease()` directly. This adds one small file read per check but guarantees coherence with the CLI without any synchronization mechanism.

Alternative considered: cache file mtime and re-read only on change. Rejected — unnecessary complexity; the file is tiny and reads are infrequent relative to normal I/O in the system.

**Decision: Use the CLI default TTL (24h) for web unlock**

Web unlock will not expose a TTL input. It writes a 24-hour lease via `write_unlock_lease(password, Duration::from_secs(86400))`. If the user needs a longer or shorter TTL, they use the CLI with `--ttl`.

Alternative considered: expose TTL as a hidden field or admin option. Rejected — scope creep; the common case is a human unlocking for a work session, 24h is appropriate.

**Decision: Keep failed-attempt rate limiting in `ServerState` only**

The backoff counter is not written to disk and resets on process restart. This matches the existing spec and is intentional — persistent rate-limit state would complicate auditing and could lock out users across restarts due to crashes.

## Risks / Trade-offs

**Disk I/O on every status poll** → Mitigation: The file is tiny (<1 KB) and status polling frequency is user-driven. Acceptable for a desktop/CLI tool.

**File deletion race on concurrent lock/unlock** → Mitigation: `write_yaml_atomic()` already uses rename-based atomic writes. Lock (deletion) is idempotent — missing file equals locked state.

**Relock from web clears the file, locking the CLI too** → This is the intended behavior. It is a breaking UX change. Document in release notes.

**Bootstrap flow (no keystore file exists)**: web bootstrap creates the keystore but must also write an initial unlock lease so the process immediately reflects unlocked state. The existing bootstrap path writes the keystore; it must also call `write_unlock_lease()` after creation.

## Migration Plan

1. Remove `KeystoreSessionState` struct and replace `keystore_state: Arc<RwLock<KeystoreSessionState>>` in `ServerState` with `keystore_failed_attempts: Arc<Mutex<FailedAttemptState>>` (or equivalent minimal struct)
2. Update `/keystore/unlock` handler to call `write_unlock_lease()` on success
3. Update `/keystore/lock` handler to call `delete_unlock_lease()` (or equivalent) on success
4. Update keystore status signal emission to read `read_unlock_lease()` instead of in-memory state
5. Update web bootstrap flow to call `write_unlock_lease()` after keystore creation
6. Delete all code paths that read from `KeystoreSessionState.unlocked_password`
7. Update `web-keychain-session-unlock` delta spec

No data migration required — the unlock file format is unchanged. Existing CLI unlock leases remain valid after upgrade.

## Open Questions

- Does `delete_unlock_lease()` already exist as a public function in `src/data/keystore.rs`, or does it need to be added alongside `write_unlock_lease()`?
- Should the `/keystore/lock` endpoint also clear the `failed_attempts` counter, or leave it intact?
