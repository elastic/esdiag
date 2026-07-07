---
status: accepted
---

# Resource governance for the shared service

`Service` mode gains job **concurrency caps** — a global cap and a per-`Owner` cap —
to keep one user (or an automated client) from starving a shared server. Backpressure
is already structural (bounded document channels + bulk count/byte limits), so no
per-job memory cap is imposed; large jobs must still succeed by slowing down, not by
rejection. Cross-job coordination of load against the shared output cluster is
explicitly **deferred**.

## Context

Today the only limiter is `keystore_rate_limit` (unlock brute-force, `User` mode).
ADR-0008 gave *visibility* isolation but not *resource* isolation, and `stats.jobs.active`
is counted but never enforced. The `Owner` (ADR-0008) now makes per-user caps
expressible. At current volume (hundreds of users, dozens of jobs/day, little overlap)
this has not been a problem — the caps are proactive insurance against rising
automation.

## Decisions

- **Global + per-`Owner` concurrent-job caps** in `Service` mode, enforced against the
  already-tracked active-job count.
- **No per-job memory cap.** Bounded channels + bulk count/byte limits already provide
  backpressure; the average job is small and large jobs must still complete. Rejecting
  large jobs is worse than letting them stream slowly.
- **Weight → concurrency is deployment-tunable policy** (with the two-axis weights of
  ADR-0017 and the registry of ADR-0005), replacing the hardcoded `buffer_unordered(5)`.

## Deferred (noted future concern)

- **Coordinated load budget against the shared output cluster.** Per-job `429` retry
  is not coordinated across concurrent jobs, so N jobs can independently hammer the one
  mandatory sink (ADR-0007). Not urgent at current volume; **revisit when concurrent-job
  overlap or automation rises** — that is the trigger to add a shared export
  concurrency/rate budget.
