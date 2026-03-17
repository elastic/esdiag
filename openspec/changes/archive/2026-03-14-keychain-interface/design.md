## Context

The current web settings modal combines two concerns: selecting an output target and creating/editing host credentials inline. Recent CLI work introduced encrypted keychain support, but the web UI does not yet provide a secure workflow for unlocking keychain access and managing host-to-secret bindings.

This change introduces a Datastar-first management flow that separates quick output switching from host/keychain maintenance:
- Footer output control remains for active target selection only.
- Host and secret maintenance live in the dedicated `/settings` management UI.
- Keychain operations are gated by a secrets password supplied by the user.
- A `Keystore` user-menu action exposes explicit unlock/lock toggling.
- Secure-host processing startup is blocked until keystore unlock succeeds.

Constraints:
- Persisted/decrypted keychain values must remain backend-only, while transient plaintext draft values are allowed in active edit state when required for submission.
- Session and rate-limit state are in-memory only and do not persist across process restarts.
- `KnownHost` records remain persisted in `hosts.yml`, while secret payloads remain in encrypted keychain storage.
- Keystore behavior must be compile-time gated behind the `keystore` cargo feature and exposed only in user mode; in `service` mode keystore routes are absent.

## Goals / Non-Goals

**Goals:**
- Provide a secure unlock step for encrypted keychain use in web sessions.
- Enable full editing of `KnownHost` fields from the dedicated Datastar management UI.
- Allow auth binding by selecting keychain entry names without exposing secret values.
- Surface authenticated keystore state with clear locked/unlocked iconography in the manager UI.
- Prevent secure diagnostic processing from starting until keystore authentication succeeds.
- Ensure keystore UI/actions are unavailable when `keystore` feature is disabled or runtime mode is `service`.
- Keep existing quick output-target switching behavior intact while removing inline host creation from that path and honoring the live CLI-defined output target when present.

**Non-Goals:**
- Changing encrypted keychain cryptography primitives or file format.
- Sending decrypted secret values or ciphertext blobs to the frontend outside transient active draft state required for submission.
- Introducing multi-user or distributed session storage semantics in this change.

## Decisions

1. Introduce explicit unlock state in server session context
   - Decision: Add an explicit keychain unlock state machine in user mode with a 12-hour in-memory session lease: `Locked` -> `Unlocked` -> `Locked`. This unlock state is intentionally a single local user-mode web session for the running process, not a multi-user or distributed session model.
   - Rationale: Prevent accidental keychain access before user intent, keep lock behavior explicit, and avoid implying stronger per-user isolation guarantees than the current local user-mode web flow provides.
   - Alternatives considered:
     - Browser cookie-backed unlock tracking: rejected because the server does not actually validate a session cookie and the extra header would imply isolation guarantees that do not exist.
     - Prompt-on-every-operation: rejected for poor UX and repetitive friction.

2. Split footer interactions from host management
   - Decision: Keep output selector focused on choosing the active output target, including a live CLI-defined target label when applicable; move host and secret CRUD into the dedicated `/settings` management UI.
   - Rationale: Clarifies intent, preserves fast switching, and avoids overloading the footer with full record authoring.
   - Alternatives considered:
     - Keep add/edit in selector dropdown: rejected as hard to scale for full record editing and keychain management.

3. Dedicated host/keychain management UI with backend-mediated secret operations
   - Decision: The `/settings` interface presents host forms and keychain entry list as separate but related sections; keychain list exposes only metadata (name, optional description/timestamps), while active drafts may carry plaintext values the user is editing for submission.
   - Rationale: Preserves security boundaries while allowing users to wire host auth to named secrets.
   - Alternatives considered:
     - Return masked secret values to frontend: rejected because even masked/derived secret data expands leakage surface.

4. Validate and persist through backend command handlers only
   - Decision: All create/update/delete operations for hosts and keychain entries go through backend validation and persistence endpoints/actions.
   - Rationale: Centralizes invariants (valid host fields, existing keychain reference, safe write ordering).
   - Alternatives considered:
     - Client-side optimistic persistence model: rejected due to higher risk of invalid or partial writes.

5. Session-duration unlock caching with explicit relock
   - Decision: Use explicit `/keystore/unlock` and `/keystore/lock` endpoints only (idempotent), with no toggle endpoint.
   - Rationale: Keeps lock lifecycle transitions explicit and easy to reason about across UI and backend.
   - Alternatives considered:
     - UI-only toggle action: rejected because implicit server transitions are harder to audit and test.

6. Make lock state visible and directly controllable from user menu
   - Decision: Add a `Keystore` menu item in the user pop-up. Clicking when locked opens password prompt (`/keystore/unlock`); clicking when unlocked asks for lock confirmation before calling `/keystore/lock`.
   - Rationale: Provides a consistent single entry point for lock lifecycle management and avoids hidden background state.
   - Alternatives considered:
     - Keystore controls only inside manager modal: rejected as too buried for pre-processing unlock needs.

7. Add secure-host processing preflight unlock gate
   - Decision: Before starting processing for hosts with auth type other than `NoAuth`, run a preflight that enforces unlocked state. If locked, present a modal password prompt and resume processing automatically after successful unlock.
   - Rationale: Prevents starting a job that is guaranteed to fail later on secret retrieval and gives immediate feedback at action time.
   - Alternatives considered:
     - Lazy failure during backend execution: rejected due to poor UX and delayed error reporting.

8. Use field-level invalidation on incorrect password submissions
   - Decision: Invalid secrets password responses map to password input invalidation state in Datastar form handling with retry allowed.
   - Rationale: Keeps errors local to user action and avoids ambiguous global toasts.
   - Alternatives considered:
     - Generic modal error banners only: rejected because they are less actionable.

9. Add availability matrix for keystore capability
   - Decision: Keystore routes are compiled only when `cfg(feature = "keystore")` is active and are not mounted in `service` mode. In both cases, `/keystore/*` resolves to HTTP 404. When keystore capability is unavailable, processing may still use a runtime-configured authenticated exporter if its credentials were provided outside local keystore storage, because there is no local unlock step to satisfy.
   - Rationale: Prevents presenting unusable controls, keeps non-keystore builds lean, and preserves valid single-target runtime configurations that already provide their own output authentication.
   - Alternatives considered:
     - Runtime-only hiding without compile-time flag: rejected because code paths still compile/ship in non-keystore builds.

10. Make backend lock state the only UI truth source
   - Decision: Track `keystore.locked: bool` and `keystore.lock_time: int` in Datastar signals as UI status fields; mutate them only through backend state transitions, including `/keystore/unlock`, `/keystore/lock`, and timeout-driven lease expiry.
   - Rationale: Prevents frontend-side drift and guarantees signal consistency with server state, even when a lease expires outside an explicit user-initiated lock request.
   - Alternatives considered:
     - Client-generated lock state transitions: rejected because it can desynchronize from backend security state.

11. Refresh session lease on keystore reads and secure host requests
   - Decision: Refresh the 12-hour session lease on any keystore read and any request sent to a secure saved host so session does not expire during processing lifecycle.
   - Rationale: Avoids mid-run expiry during long processing operations.
   - Alternatives considered:
     - Fixed expiry without touch-on-read: rejected due to avoidable interruptions.

12. Apply in-memory unlock-rate limiting and structured logging
   - Decision: Allow 3 failed password attempts with no delay, then apply lockout delay of +5 minutes per additional failure (4th onward), capped at 60 minutes; no persistence across restart. Log successful auth and timeout closures as INFO, failed auth as WARN.
   - Rationale: Balances brute-force resistance and operational simplicity.
   - Alternatives considered:
     - Persistent lockout store: rejected for this phase to keep state ephemeral.

13. Use explicit bootstrap flow for missing keystore state
   - Decision: If the keystore file is missing, user-mode web flows initialize the bootstrap modal so the user explicitly creates or migrates the keystore with a chosen password, rather than auto-creating storage at startup.
   - Rationale: Keeps keystore creation intentional and aligned with the password-confirmation UX already used for migration/bootstrap.
   - Alternatives considered:
     - Auto-create an empty keystore on startup: rejected because it creates encrypted storage before the user has explicitly confirmed password setup.

## Risks / Trade-offs

- [Risk] Session unlock state lingers longer than intended -> Mitigation: enforce 12-hour TTL with touch-on-read refresh and explicit relock endpoint.
- [Risk] Race conditions between host edits and output-target save -> Mitigation: serialize writes to `settings.yml` and `hosts.yml` with deterministic ordering.
- [Risk] UX complexity from separate unlock + management flows -> Mitigation: progressive disclosure (prompt for password only when keychain-backed actions are attempted).
- [Risk] Partial migration from old inline host creation path -> Mitigation: remove add-new controls from output selector in one release and cover with regression tests.
- [Risk] Lock icon state drifts from server session truth -> Mitigation: drive icon from backend lock-status endpoint/action and refresh after unlock/lock attempts.
- [Risk] Processing action race when user unlocks concurrently -> Mitigation: re-check unlocked state server-side immediately before enqueue/start.
- [Risk] Feature-flag and mode checks diverge across UI/backend -> Mitigation: centralize availability helper and test both feature-enabled and feature-disabled builds.

## Migration Plan

1. Update footer/template controls to reduce output selector scope, include the live CLI-defined output target, and remove inline host creation.
2. Introduce backend unlock/session primitives and route/actions for lock/unlock status.
3. Implement `/settings` management actions for `KnownHost` CRUD and keychain metadata listing.
4. Add keystore user-menu flow backed only by idempotent `/keystore/unlock` and `/keystore/lock`.
5. Add manager-page lock/unlock icon and Datastar signal hydration from backend lock state.
6. Add compile-time `keystore` feature gates and runtime `service` mode route exclusion (`/keystore/*` -> 404).
7. Add secure-host processing preflight unlock gate with retry-on-invalid-password behavior (401 on invalid password).
8. Add session lease refresh on keystore reads and secure host requests.
9. Add unlock rate limiting and structured lock/auth logging.
10. Implement missing-keystore bootstrap lifecycle in the web UI.
11. Update output-target save flow to support saved host names plus the live CLI-defined output target.
12. Add integration tests for:
   - lock/unlock behavior,
   - keystore availability matrix (feature on/off, service mode),
   - HTTP semantics (`401` invalid password, `404` unavailable routes),
   - secure-host classification (`NoAuth` excluded),
   - session lease refresh on secure host processing lifecycle,
   - rate-limit backoff curve and cap,
   - startup behavior (create missing, unreadable file handling),
   - user-menu keystore toggle and lock confirmation,
   - secure-host preflight unlock gate with field invalidation on bad password,
   - host edits with keychain references,
   - guarantee that persisted keychain values remain absent from frontend payloads outside transient active drafts.
13. Rollback strategy: keep schema-compatible `hosts.yml`/keychain data; revert web routes/templates while retaining stored records.

## Open Questions

- Should rate-limit delays be communicated as exact remaining time in the unlock form UX, or generic retry messaging?
