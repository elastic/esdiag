# Design

Full rationale, rejected alternatives, and consequences live in
**`docs/adr/0007-separate-authentication-from-runtime-mode.md`**,
**`docs/adr/0008-per-user-isolation-is-structural.md`**, and
**`docs/adr/0018-resource-governance-for-the-shared-service.md`**; this design covers only
the implementation approach for the shared-service multi-user story.

## Context

`Service` mode is already the shared, locked-down deployment archetype, but three axes are
under-specified in code:

- **Auth is a mode alias.** `ServerPolicy::requires_iap_headers()` is `mode == Service`
  (`src/server/mod.rs:140`) and is the sole gate at `mod.rs:414`/`564`/`916`. Auth cannot
  vary from tenancy.
- **Isolation is opt-in.** `event_visible_to_user` (`mod.rs:1370`) special-cases only
  `ServerEvent::TargetedSignals`; all other variants (`Signals`, `Template`, `JobFeed`,
  `ReplaceSelector`, …) hit `_ => true`. `RetainedBundle.owner` exists (`mod.rs:525`) but
  `JobRequest`/`Job` have no owner.
- **No resource caps.** `stats.jobs.active` is incremented/decremented (`mod.rs:558`) but
  never checked; collect concurrency is a hardcoded `buffer_unordered(5)`.

## Approach

### Authentication as a separate axis (ADR-0007)

- Introduce an `AuthProvider` abstraction (`GoogleIap` today; extensible to another IAP,
  Elastic Cloud SSO, or `None`) configured independently of `RuntimeMode`. Replace the
  `requires_iap_headers()` mode-derived gate with a "which provider" decision on
  `ServerPolicy`; `resolve_user_email` resolves identity via the configured provider.
- **Keep the tenancy⇒capability bundling.** `Service` still forbids a shared keystore and
  user-editable exporter and forces the single startup exporter — only the auth clause is
  unbundled from the mode contract.
- Authentication has two jobs: *access control* (gate a shared instance) and *identity
  provenance* — the resolved identity populates `Identifiers` (user, account) on bundles
  and authorizes outbound `Send` to the support portal, in either mode.

### Structural isolation (ADR-0008)

- Every `ServerEvent` carries an `owner`; add `is_broadcast()`, true only for the
  aggregate-stats class. `event_visible_to_user` becomes
  `event.owner == user || event.is_broadcast()` — **default deny**, broadcast is the
  explicit exception. `TargetedSignals` collapses into this general rule.
- The aggregate-stats snapshot the `/events` stream sends on subscribe (`mod.rs:1360`)
  stays broadcast — it is the sole shared, server-wide view.

### Execution ownership (ADR-0008)

- `Owner` = the authenticated user who *executed* the job, extending
  `RetainedBundle.owner` onto `JobRequest`/`Job`/`JobID`. Saved-job *definitions* need no
  owner (authoring is a `User`-mode keystore concern).
- A job's emitted events auto-target its owner (no per-emission opt-in). In
  `spawn_sub_processors` the owner is threaded to each child diagnostic alongside the
  existing `parent_id`/platform inheritance, so children's events stay owner-scoped.

### Resource governance (ADR-0018)

- Add a **global** concurrent-job cap and a **per-`Owner`** cap in `Service` mode,
  enforced against the already-tracked active-job count (`stats.jobs.active`). The
  weight→concurrency mapping replacing the hardcoded `buffer_unordered(5)` is
  deployment-tunable policy.
- **No per-job memory cap.** Bounded document channels + bulk count/byte limits already
  provide backpressure; a large job must complete by streaming slowly, never be rejected.

## Invariants

- Exactly one `RuntimeMode`; capability is a total function of it — auth cannot re-grant a
  capability the mode forbids.
- Auth provider is independent of mode: `Service` MAY run with `None` (local testing);
  `User` MAY authenticate (Cloud SSO).
- Every non-stats `ServerEvent` has an owner; visibility defaults to deny. `is_broadcast()`
  is true only for the aggregate-stats class.
- A child diagnostic's owner equals its parent's.
- Concurrency caps apply to `Service` mode only and never reject a job for size.

## Deferred (noted future concern)

- **Coordinated load budget against the shared output cluster.** Per-job `429` retry is
  not coordinated across concurrent jobs, so N jobs can independently hammer the one
  mandatory sink. Per ADR-0018 this is **not implemented now** (not urgent at current
  volume); the **trigger to add a shared export concurrency/rate budget is rising
  concurrent-job overlap or automation**. Captured here and as a deferred requirement in
  the `web-runtime-modes` delta so it is not silently lost.

## Risks

- **Blast radius on `ServerEvent`.** Adding a mandatory owner touches every event
  constructor; mitigated by auto-targeting from the job's owner so most call sites do not
  set it explicitly, and by the default-deny test at the visibility boundary.
- **Auth regressions.** Decoupling the mode gate risks an unauthenticated `Service`
  deployment; mitigated by keeping provider configuration explicit at startup and logging
  the resolved provider (as the current policy line already logs `requires_iap_headers`).
- **Cap tuning.** Caps set too low throttle legitimate use; they are deployment-tunable
  policy, defaulting to values above observed per-user volume.
