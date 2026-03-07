## Why

The current web configuration flow is optimized for quick target switching, but it is not suitable for full host record maintenance now that encrypted keychain-backed authentication has landed in CLI. We need a form-based Datastar workflow that lets users safely manage `KnownHost` entries and keychain references from the UI without exposing secret values to the browser.

## What Changes

- Add a Datastar-powered host/keychain manager modal launched by a new `Edit Hosts` action in the footer.
- Replace the current footer output interaction behavior so the output selector no longer creates hosts inline.
- Require a secrets password in the web flow to unlock encrypted keychain operations, with optional in-memory session retention for the active user session.
- Add keystore lock-state UX controls: a locked/unlocked glyph on the manager page and a `Keystore` item in the user menu that toggles unlock/lock confirmation flows.
- Gate all keystore functionality behind a compile-time `keystore` feature flag and disable keystore UI affordances when the feature is off or the app runs in `service` mode.
- In user mode, use a 12-hour session lease refreshed by keystore-backed host activity; apply in-memory failed-password backoff and explicit `/keystore/unlock` and `/keystore/lock` lifecycle endpoints.
- Support full CRUD-style editing of `KnownHost` fields in `hosts.yml`, including selecting authentication references from keychain entry names.
- Gate processing startup for secure hosts behind successful keystore authentication, with inline password form validation errors on incorrect credentials.
- Add backend-only handling for secret material so frontend payloads include only keychain entry metadata (for example, secret names) and never decrypted values.

## Capabilities

### New Capabilities
- `web-keychain-session-unlock`: Web workflow for providing and caching the secrets password in session scope to authorize encrypted keychain operations.
- `web-hosts-keychain-manager`: Datastar modal UX and backend APIs for editing `KnownHost` records and binding auth fields to keychain entry names.
- `web-secure-processing-gate`: Pre-processing guard that blocks secure-host diagnostic runs until keystore unlock succeeds.

### Modified Capabilities
- `desktop-settings`: Footer target/output interaction model changes from inline host creation to an explicit `Edit Hosts` entry point and manager modal launch behavior.

## Impact

- Affected specs: `desktop-settings` (modified), plus new specs for web keychain unlock, secure-processing gating, and host/keychain management.
- Affected code areas: web handlers/routes, Datastar templates/signals/actions, host persistence (`hosts.yml`), keychain integration layer, and session state handling.
- Build/runtime impact: feature-conditional compilation paths for keystore backend/UI and runtime mode checks to suppress keystore affordances in `service` mode.
- API impact: `/keystore/*` routes return `404` when keystore capability is unavailable; `/keystore/unlock` returns `401` for invalid password.
- Security impact: enforce backend-only secret handling and avoid transmitting decrypted secrets to frontend state or responses.
- UX impact: adds a two-stage workflow (unlock secrets, then edit hosts/keychain links) that prioritizes safety and explicit user intent.
