## Why

The current web configuration flow is optimized for quick target switching, but it is not suitable for full host record maintenance now that encrypted keychain-backed authentication has landed in CLI. We need a Datastar workflow that lets users safely manage `KnownHost` entries and keychain references from the UI while keeping the footer focused on target selection and the dedicated `/settings` interface focused on record maintenance.

## What Changes

- Add a Datastar-powered host/keychain management interface on `/settings` for host and secret CRUD.
- Replace the current footer output interaction behavior so the output selector no longer creates hosts inline and can surface the live CLI-defined output target alongside saved hosts.
- Require a secrets password in the web flow to unlock encrypted keychain operations, with a 12-hour in-memory session lease for the active user session.
- Add keystore lock-state UX controls: a locked/unlocked glyph on the manager page and a `Keystore` item in the user menu that toggles unlock/lock confirmation flows.
- Gate all keystore functionality behind a compile-time `keystore` feature flag and disable keystore UI affordances when the feature is off or the app runs in `service` mode.
- In user mode, use a 12-hour session lease refreshed by keystore-backed host activity; apply in-memory failed-password backoff and explicit `/keystore/unlock` and `/keystore/lock` lifecycle endpoints.
- When no keystore exists, use the explicit bootstrap/migration modal flow rather than silently creating storage at startup.
- Support full CRUD-style editing of `KnownHost` fields in `hosts.yml`, including selecting authentication references from keychain entry names.
- Gate processing startup for secure hosts behind successful keystore authentication, with a modal password prompt that resumes the original action after successful unlock and field validation errors on incorrect credentials.
- Keep persisted keychain material backend-only while allowing transient plaintext draft values in active edit state when needed to submit a user-authored secret to the backend.

## Capabilities

### New Capabilities
- `web-keychain-session-unlock`: Web workflow for providing and caching the secrets password in session scope to authorize encrypted keychain operations.
- `web-hosts-keychain-manager`: Datastar management UI and backend APIs for editing `KnownHost` records and binding auth fields to keychain entry names.
- `web-secure-processing-gate`: Pre-processing guard that blocks secure-host diagnostic runs until keystore unlock succeeds.

### Modified Capabilities
- `desktop-settings`: Footer target/output interaction model changes from inline host creation to a selector that supports saved hosts plus the live CLI-defined output target.

## Impact

- Affected specs: `desktop-settings` (modified), plus new specs for web keychain unlock, secure-processing gating, and host/keychain management.
- Affected code areas: web handlers/routes, Datastar templates/signals/actions, host persistence (`hosts.yml`), keychain integration layer, and session state handling.
- Build/runtime impact: feature-conditional compilation paths for keystore backend/UI and runtime mode checks to suppress keystore affordances in `service` mode.
- API impact: `/keystore/*` routes return `404` when keystore capability is unavailable; `/keystore/unlock` returns `401` for invalid password.
- Security impact: keep persisted/decrypted keychain storage backend-only while constraining any plaintext secret values in the browser to transient active draft state only.
- UX impact: adds an explicit unlock workflow for keystore-backed actions while keeping host/secret editing in the dedicated `/settings` experience.
