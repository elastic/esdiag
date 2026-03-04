## Context

The web interface currently assumes a single local-user workflow with persisted settings and writable local artifacts. With Tauri desktop distribution and shared `serve` deployments, this assumption no longer holds: some deployments run as shared, ephemeral services where local credential and host persistence is invalid.

This change introduces explicit runtime mode semantics for web execution only:
- `service` mode: shared/multi-user, IAP-authenticated requests, minimal mutable preferences, startup-defined exporter, and no local artifact persistence.
- `user` mode: single-user local execution, saved credentials and writable `hosts.yml`-style artifacts, richer preferences, runtime exporter changes, and no external auth requirement.

CLI collection/execution contracts remain unchanged.

## Goals / Non-Goals

**Goals:**
- Make runtime mode an explicit part of web server startup and request handling.
- Enforce mode-specific behavior for auth, settings persistence, and exporter mutability.
- Keep one web codebase that supports both `serve` and desktop-hosted variants through shared mode-aware abstractions.
- Preserve existing CLI behavior and defaults outside the web runtime path.

**Non-Goals:**
- Redesigning collector/processor type-state flows used by CLI workflows.
- Introducing new external identity providers beyond trust of already-validated IAP headers.
- Changing diagnostic API selection, receiver pipelines, or exporter internals unrelated to mode gating.

## Decisions

### 1) Add explicit `RuntimeMode` to web bootstrap

Decision:
- Introduce a `RuntimeMode` enum (`Service`, `User`) resolved at web startup (`serve` and desktop wrapper paths).
- Resolve mode using hybrid precedence:
  - explicit `--mode` CLI argument override,
  - `ESDIAG_MODE` environment variable fallback,
  - mode-specific default when neither is provided.
- Store mode in shared web server state and pass it to UI/state handlers.

Rationale:
- Centralizes behavior gating and avoids repeated environment checks.
- Keeps mode selection orthogonal to existing CLI command behavior.

Alternatives considered:
- Infer mode from environment variables only at each handler call. Rejected due to drift and weak testability.
- CLI-argument-only mode selection. Rejected due to operational friction for containerized deployments where env injection is easier than command overrides.
- Split into separate binaries. Rejected because it duplicates web code paths.

### 2) Define a mode policy boundary for auth and persistence

Decision:
- Add a web-focused policy abstraction (for example `RuntimeModePolicy`) with mode-specific implementations.
- Policy covers:
  - authentication source resolution (`service`: IAP headers; `user`: none by default),
  - local artifact read/write allowances (`service`: deny local `hosts.yml`/similar I/O; `user`: allow),
  - settings surface (`service`: minimal; `user`: full),
  - exporter mutability (`service`: fixed; `user`: runtime configurable).

Rationale:
- Makes behavior explicit and testable without scattering `if mode == ...` checks.
- Preserves existing exporter/receiver/processor trait boundaries by putting mode checks at web orchestration boundaries instead of core processing traits.

Alternatives considered:
- Inline mode conditionals in each endpoint. Rejected due to maintenance risk and inconsistent enforcement.

### 3) Keep existing type-state processing lifecycle unchanged

Decision:
- No new processor type-state transitions are introduced for CLI or collection internals.
- Web runtime transitions are limited to configuration/auth state:
  - `Boot` -> `Configured(mode)` at startup.
  - For `service`: `Configured(Service)` -> `AuthenticatedRequest` (per request via IAP headers) -> `ExecuteWithFixedExporter`.
  - For `user`: `Configured(User)` -> `LocalUserSession` -> `ExecuteWithMutableExporter`.

Rationale:
- Satisfies requirement to differentiate web behavior while protecting proven collection/processor state machines.
- Limits blast radius and regression risk for non-web execution paths.

### 4) Preserve existing IAP header contract and add mode observability logging

Decision:
- Keep the existing identity-aware-proxy header contract unchanged for this spec.
- Add runtime mode visibility in logs:
  - always include mode in the initial startup log message (`Starting ${mode}-mode server on port ${port_number}`),
  - expose mode in additional diagnostics when `LOG_LEVEL` is `debug` or more verbose.

Rationale:
- Avoids cross-team integration risk by not changing the upstream IAP header contract in this change.
- Improves operability and incident debugging by making active runtime mode explicit at startup and in debug diagnostics.

Alternatives considered:
- Redefine or extend IAP headers in this change. Rejected to keep scope focused on runtime mode behavior.
- Keep mode logging implicit. Rejected because explicit logging is useful for support/debug workflows.

## Risks / Trade-offs

- [Misconfigured mode causes incorrect persistence/auth behavior] -> Add startup validation and log effective mode plus policy flags.
- [IAP header trust assumptions could be bypassed in non-IAP deployments] -> Require explicit `service` mode enablement and reject requests lacking required identity headers.
- [User confusion from mode-limited settings UI] -> Render mode-aware settings UI with clear read-only/disabled explanations.
- [Shared code path complexity increases] -> Isolate mode policy behind trait interface and add integration tests per mode.

## Migration Plan

- Add mode configuration input for web startup paths (`serve`, desktop host wrapper).
- Default behavior:
  - `serve`: explicit configuration required (or documented default) to avoid accidental auth/persistence assumptions.
  - desktop app: defaults to `user` mode unless explicitly overridden by deployment packaging.
- Introduce policy-wrapped settings and host persistence services.
- Add mode-aware UI state endpoints and exporter update guards.
- Rollback strategy: revert to prior single-mode behavior by forcing `user` mode policy while retaining inert config plumbing.

## Open Questions

- Encryption-at-rest for stored user-mode secrets is desired, but requires an additional dedicated secrets-manager spec and implementation plan outside this change.
