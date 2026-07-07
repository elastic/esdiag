# Tasks

## 1. Authentication axis (ADR-0007)
- [ ] 1.1 Introduce an `AuthProvider` abstraction (`GoogleIap`, extensible to other IAP / Cloud SSO, and `None`) configured independently of `RuntimeMode` at startup.
- [ ] 1.2 Replace the `requires_iap_headers()` mode gate (`src/server/mod.rs:140`, and its use at `:414`, `:564`, `:916`) with a provider-driven authentication decision on `ServerPolicy`.
- [ ] 1.3 Route `resolve_user_email` through the configured provider; support `Service` + `none` for local testing and reject unauthenticated requests when a provider requires it.
- [ ] 1.4 Keep the tenancy⇒capability lockdown a total function of mode (no shared keystore, no user-editable exporter, single startup exporter) — unaffected by auth configuration.
- [ ] 1.5 Populate `Identifiers` (user, account) from the resolved identity in both modes; thread it toward `Send` authorization.
- [ ] 1.6 Log the resolved auth provider in the startup policy line (as `requires_iap_headers` is logged today).

## 2. Structural isolation (ADR-0008)
- [ ] 2.1 Add an `Owner` field to every `ServerEvent` variant (`mod.rs:1219`) and update the event constructors; fold `TargetedSignals` into the general owner-scoped model.
- [ ] 2.2 Add `is_broadcast()`, true only for the aggregate-stats class.
- [ ] 2.3 Change `event_visible_to_user` (`mod.rs:1370`) to `event.owner == user || event.is_broadcast()` (default deny).
- [ ] 2.4 Keep the initial stats snapshot on `/events` subscribe (`mod.rs:1360`) in the broadcast class; ensure per-user feeds/signals are not broadcast.

## 3. Execution ownership (ADR-0008)
- [ ] 3.1 Extend `RetainedBundle.owner` (`mod.rs:525`) onto `JobRequest`/`Job`/`JobID` = the authenticated executing user.
- [ ] 3.2 Auto-target a job's emitted events to its owner (no per-emission opt-in).
- [ ] 3.3 Thread the owner through `spawn_sub_processors` alongside `parent_id`/platform inheritance so child-diagnostic events stay owner-scoped.
- [ ] 3.4 Confirm saved-job definitions need no owner; resolve execution owner at run time.

## 4. Resource governance (ADR-0018)
- [ ] 4.1 Enforce a global concurrent-job cap in `Service` mode against the tracked `stats.jobs.active` count (`mod.rs:558`).
- [ ] 4.2 Enforce a per-`Owner` concurrent-job cap; reject or defer jobs that would exceed either cap.
- [ ] 4.3 Replace the hardcoded `buffer_unordered(5)` (`src/processor/*/collector.rs`) with a deployment-tunable weight→concurrency mapping.
- [ ] 4.4 Do NOT add a per-job memory cap; verify large jobs still complete under channel + bulk backpressure.
- [ ] 4.5 Record the deferred coordinated output-cluster load budget (and its rising-overlap trigger) without implementing it.

## 5. Verification
- [ ] 5.1 Test auth: `Service` + `none` accepts anonymous; provider-required rejects unauthenticated; identity populates `Identifiers`.
- [ ] 5.2 Test capability lockdown holds under every auth configuration in `Service` mode.
- [ ] 5.3 Test owner-scoped delivery: user A's non-stats events never reach user B; stats reach all.
- [ ] 5.4 Test child diagnostic inherits parent owner and its events stay owner-scoped.
- [ ] 5.5 Test caps: per-owner and global limits block excess concurrency; a large job is admitted and completes.
- [ ] 5.6 Confirm the delta-spec scenarios in `specs/web-runtime-modes/spec.md` and `specs/web-event-streaming/spec.md` are covered.
