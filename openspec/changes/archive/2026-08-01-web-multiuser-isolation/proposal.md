## Why

In `Service` mode the web server is shared, but its multi-user story is incomplete on
three fronts. (1) Authentication is **welded to the mode** — `requires_iap_headers()`
is literally `mode == Service` — so `Service` cannot be run locally without hand-injecting
a Google IAP header, cannot sit behind a different IAP, and `User` mode can never
authenticate to populate provenance or authorize `Send`. (2) Isolation is **opt-in**:
`event_visible_to_user` scopes only `ServerEvent::TargetedSignals` and every other
variant falls through to `_ => true`, broadcasting one user's job feed and UI updates to
every connected browser. (3) There is **no resource isolation** — `stats.jobs.active` is
counted but never enforced, so one user or automated client can starve the shared server.
Rationale: **ADR-0007** (auth axis), **ADR-0008** (structural isolation), **ADR-0018**
(resource governance).

## What Changes

- **Separate authentication from runtime mode** into a pluggable, provider-agnostic axis
  (Google IAP today; other IAP or Elastic Cloud SSO later; or none). Mode still bundles
  tenancy with capability — `Service` keeps the lockdown (no shared keystore, no
  user-editable exporter, single startup exporter, all processed diagnostics to the one
  shared cluster) — but whether and how requests authenticate is configured independently.
  Auth serves both **access control** and **identity provenance** (populates
  `Identifiers`, authorizes outbound `Send`).
- **Invert isolation from opt-in to opt-out.** Every `ServerEvent` carries an `Owner` and
  is visible only to that owner by default; `event_visible_to_user` becomes
  `event.owner == user || event.is_broadcast()`. Only the aggregate-`stats` class
  (processing state, diagnostics processed, document count) broadcasts to all users.
- **Attach `Owner` to the execution.** `Owner` is the authenticated user who *executed* a
  job (distinct from saved-job authorship), extending the existing `RetainedBundle.owner`
  onto the job execution. A job's events auto-target its owner, and the owner propagates
  to child diagnostics as they are spawned — isolation by construction, not by remembering.
- **Add job concurrency caps** in `Service` mode: a global cap and a per-`Owner` cap,
  enforced against the tracked active-job count. No per-job memory cap (bounded document
  channels + bulk count/byte limits already provide backpressure; large jobs must still
  succeed by slowing, not by rejection).
- **BREAKING (internal):** the `requires_iap_headers()` mode gate is replaced by an
  authentication-provider decision; every non-stats `ServerEvent` gains a mandatory owner
  (the old broadcast-by-default behavior is removed).

## Capabilities

### New Capabilities

- _(none — this modifies existing capabilities)_

### Modified Capabilities

- `web-runtime-modes`: unbundle authentication from the mode enum into a pluggable
  provider axis; retain the tenancy⇒capability bundling; add `Service`-mode global +
  per-`Owner` job concurrency caps.
- `web-event-streaming`: make event visibility owner-scoped by default with an
  aggregate-stats broadcast allowlist; attach `Owner` to the execution, auto-target a
  job's events to its owner, and propagate the owner to child diagnostics.

## Impact

- **Web UI (`Service` mode):** `event_visible_to_user` / `broadcast_receiver_stream`
  (`src/server/mod.rs:1370`, `1312`); the `ServerEvent` enum and its constructors
  (`mod.rs:1219`); `requires_iap_headers()` and the auth middleware gate (`mod.rs:140`,
  `414`, `564`, `916`); `RetainedBundle.owner` extended onto `JobRequest`/`Job`;
  `stats.jobs.active` promoted from counted to enforced (`mod.rs:558`); the hardcoded
  `buffer_unordered(5)` collect concurrency (`collector.rs`).
- **Core:** owner threaded through `spawn_sub_processors` alongside `parent_id`/platform
  inheritance so child-diagnostic events stay owner-scoped; authenticated identity flows
  into `Identifiers`.
- **CLI:** unaffected — runtime-mode and isolation behavior is web-only (existing CLI
  behavior-isolation requirement is preserved).
- **Deferred (not implemented here):** a coordinated load budget against the shared
  output cluster — see design.md.
