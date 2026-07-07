---
type: Reference
title: "Per-user isolation is structural: owner-scoped by default, aggregate stats broadcast"
status: accepted
tags: [repository, adr]
---

# Per-user isolation is structural: owner-scoped by default, aggregate stats broadcast

The web UI's multi-user isolation is inverted from opt-in to opt-out. Every UI event
is scoped to an **owner** and visible only to that user; a small, explicit set of
**aggregate stats** events (processing state, diagnostics processed, document count —
the `stats` data) is broadcast to all connected users. A job is owned by the user who
**executes** it, distinct from saved-job authorship.

## Problem

Isolation is opt-in today: `event_visible_to_user` filters only
`ServerEvent::TargetedSignals`; every other variant (`Template`, `JobFeed`,
`Signals`, …) falls through to `_ => true` and broadcasts to all browsers. And
`JobRequest` has no owner (only `RetainedBundle` does). So in `Service` mode one
user's job feed and UI updates leak to everyone, and correctness rests on developers
remembering to use `targeted_*`.

## Considered options

- **Opt-in targeting (today).** Rejected: leaky by default; a forgotten
  `targeted_*` silently cross-delivers another user's UI.
- **Scoped-by-default, broadcast allow-list (chosen).** Default deny; broadcast is
  the explicit exception, reserved for server-wide aggregate stats.

## Consequences

- **Ownership attaches to the execution.** `JobRequest` (and the running `Job` /
  `JobID` of ADR-0004) gains an `owner` = the executing user, extending the existing
  `RetainedBundle.owner`. Saved-job *definitions* need no owner for isolation —
  authoring is a `User`-mode-only, keystore concern, separate from execution.
- **`event_visible_to_user` flips** to roughly `event.owner == user ||
  event.is_broadcast()`, where `is_broadcast()` is true only for the aggregate-stats
  class. Every non-stats `ServerEvent` carries an owner.
- **A job's events auto-target its owner** — no per-emission opt-in, so new UI code
  is isolated by construction rather than by remembering to scope it.
- **`Owner` propagates to child diagnostics.** When a diagnostic spawns included
  children (`spawn_sub_processors`), the owner is threaded to each child alongside the
  existing `parent_id`/platform inheritance, so a child's events are owner-scoped too.
- **Aggregate stats stay a shared, server-wide view** — all users see total
  throughput; this is intentional and the sole broadcast category.
