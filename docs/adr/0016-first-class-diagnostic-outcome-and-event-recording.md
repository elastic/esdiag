---
type: Reference
title: "Diagnostics have a first-class outcome; the report records all events"
status: accepted
tags: [repository, adr]
---

# Diagnostics have a first-class outcome; the report records all events

A diagnostic gains a single first-class `DiagnosticOutcome` (`Complete | Partial |
Failed | Skipped`) that applies to **any** diagnostic — parent or child — replacing
the child-only `IncludedDiagnosticOutcome` and the top-level's counts-only model. The
diagnostic report becomes the persisted record of **all error/warning/success-level
events** (with source and reason), not just aggregate counts, so failures are
*collected* rather than dropped to `tracing` logs.

## Problem

Today success is never defined — the report carries only counts (`docs`, `errors`)
with no verdict — and the pipeline degrades silently: `ProcessorSummary::merge(Err)`
drops a whole-source failure to a `tracing::warn!`, receiver errors emit empty
summaries, and "what failed and why" survives only in logs. Children already have an
outcome; the parent does not.

## Considered options

- **Counts-only + `tracing` logs (today).** Rejected: silent degradation, no verdict,
  log-scraping to learn what failed, parent/child inconsistency.
- **First-class outcome + recorded events (chosen).** The report is the source of
  truth; the outcome is derived from the recorded events.

## Consequences

- **One `DiagnosticOutcome` for parent and child.** `IncludedDiagnosticOutcome`
  unifies into it. The outcome is derived: any collected failure/partial capture →
  `Partial`; total failure → `Failed`; unsupported → `Skipped`; all good → `Complete`.
- **`Skipped` must distinguish *by-design* from *not-implemented*.** Per ADR-0019
  these are opposite meanings behind the same skip today: out-of-scope by design
  (e.g. platform-level API collection) vs work-in-progress (Kibana processing, Agent
  processing/PR293). The outcome should carry which, so a skip reads as "nothing to
  do here" vs "TODO" rather than an undifferentiated non-result.
- **Failures are collected, not logged-and-dropped.** `ProcessorSummary::merge(Err)`
  records a failure event (source + reason + severity) into the report instead of
  warning. Collection *and* processing failures are persisted.
- **The report is the persisted source of truth for the job feed.** The owner-scoped
  job feed (ADR-0008) renders collection/processing failures from the persisted
  report; the CLI exit code and WebUI status read the same single outcome.
- **Status is two-level.** *Request/transport* status (the bulk call's HTTP code) is
  distinct from *document-level* status — an Elasticsearch `_bulk` request can return
  transport `200` while individual docs are rejected (`409 conflict`, `429`). The
  per-doc `status_counts` histogram is authoritative for document outcomes and feeds
  the `Partial` verdict; the scalar request code must not collapse mixed doc statuses.
- **HTTP `0` is reserved for non-HTTP exporters** (file, stream, directory), not used
  to mean "mixed" — fixing the current `merge`-to-`0` conflation.
