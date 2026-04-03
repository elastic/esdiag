## Why

The web server currently maintains a separate in-memory unlock state that is scoped to the running process, meaning unlocking from the web UI does not persist across restarts and cannot be shared with the CLI or other clients. For the Agent Skill to work from desktop apps like Claude, the keystore must be unlocked via CLI or the ESDiag Desktop app — the web-only session unlock is invisible to the agent, making the web unlock useless in that flow.

## What Changes

- **BREAKING**: Remove the process-scoped in-memory unlock session from the web server entirely
- Web unlock actions (`/keystore/unlock`) will now write the same `keystore.unlock` file as `esdiag keystore unlock`
- Web lock actions (`/keystore/lock`) will now delete the `keystore.unlock` file, same as `esdiag keystore lock`
- Web keystore state (locked/unlocked) is now read solely from the file-based unlock lease, not in-memory session state
- Session lease TTL is now governed by the CLI unlock file TTL (default 24h), not a separate 12-hour in-memory timer
- Rate limiting for failed unlock attempts remains in-memory (process-scoped, no file persistence) — this is intentional

## Capabilities

### New Capabilities

*(none)*

### Modified Capabilities

- `web-keychain-session-unlock`: The session-scoped in-memory unlock is replaced by file-based unlock; the web runtime now creates, refreshes, and deletes the CLI unlock file

## Impact

- **Web server**: `AppState` unlock state moves from in-memory `Arc<Mutex<SessionState>>` to reading/writing the `keystore.unlock` file on every lock-state check and transition
- **`/keystore/unlock` route**: Now writes `keystore.unlock` on success instead of updating in-memory state
- **`/keystore/lock` route**: Now deletes `keystore.unlock` instead of clearing in-memory state
- **Keystore status signal polling**: Reads unlock file rather than in-memory state
- **`web-keychain-session-unlock` spec**: Multiple requirements change; the "Session-Scoped Unlock Retention" requirement is inverted
- **`cli-keystore-lifecycle` spec**: No changes — the unlock file contract is already defined there
- **`host-secret-store` spec**: No changes — credential resolution already reads the unlock file
