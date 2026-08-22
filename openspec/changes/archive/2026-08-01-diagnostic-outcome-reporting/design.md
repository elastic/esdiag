# Design

Full rationale, rejected alternatives, and consequences live in
**`docs/adr/0016-first-class-diagnostic-outcome-and-event-recording.md`**; this design
covers only the implementation approach. The `Skipped` by-design vs not-implemented
split follows **ADR-0019**, and the job-feed consumer is the owner-scoped feed of
**ADR-0008**.

## Context

`DiagnosticReport` (`src/processor/diagnostic/report.rs`) aggregates counts through
`ProcessorSummary`, `BatchStats`, and `BatchResponse`. Two failure paths lose
information today:

- `ProcessorSummary::merge(Err)` and `add_child(Err)` (`report.rs:487`, `:501`) drop a
  whole-source `Err` to `tracing::warn!` — the failure never reaches the report.
- `BatchResponse::merge` (`report.rs:420`) collapses mixed request `status_code`s to
  `0`, conflating "non-HTTP exporter" with "mixed doc statuses", while the per-doc
  `status_counts` histogram already holds the authoritative document outcomes.

Children carry `IncludedDiagnosticOutcome` (`src/processor/mod.rs:101`) with
`Completed | Skipped | Failed`; the parent has no equivalent verdict.

## Approach

- **Derive, don't assign.** Add `DiagnosticOutcome` (`Complete | Partial | Failed |
  Skipped`) computed from the report's recorded events — never set imperatively at
  scattered call sites. Derivation: any recorded failure/partial-capture event →
  `Partial`; a total failure (nothing collected) → `Failed`; an unsupported diagnostic
  → `Skipped`; otherwise `Complete`.
- **Collect events.** Give the report an event log — each entry carries a severity
  (`error | warning | success`), a source (which data source / processor / exporter),
  and a reason. `ProcessorSummary::merge(Err)` / `add_child(Err)` record an `error`
  event instead of warning; collection and processing failures both land here.
- **Unify child and parent.** Replace `IncludedDiagnosticOutcome` with
  `DiagnosticOutcome`; a child's outcome is derived from its own child report exactly
  as the parent's is. `Skipped` gains a discriminator for by-design vs
  not-implemented.
- **Two-level status.** Keep the scalar request `status_code` as the transport verdict
  of a single call, but make `status_counts` (per-doc HTTP code histogram) the
  authoritative source for document outcomes and the `Partial` derivation. Stop
  `merge` collapsing mixed request codes to `0`; reserve `0` exclusively for non-HTTP
  exporters (`file`, `stream`, `directory`).
- **One source of truth.** The persisted report is what the job feed renders and what
  the CLI exit code and WebUI status read — no second path.

## Invariants

- Every diagnostic report has exactly one `DiagnosticOutcome`, and it equals the value
  derived from that report's recorded events (no way to set an outcome inconsistent
  with the events).
- A collection or processing failure is a recorded event, never only a log line.
- `status_code == 0` ⟺ a non-HTTP exporter; a mixed set of HTTP request codes is never
  represented as `0`.
- Document-level outcomes come from `status_counts`; the scalar request code does not
  override them (a `200` request with rejected docs is `Partial`).
- Child and parent use the same outcome type; child derivation reads the child report.

## Risks

- **Blast radius on the child type.** Removing `IncludedDiagnosticOutcome` touches the
  fan-out, job-feed rendering, CLI summaries, and the synchronous API result mapping —
  mitigated by keeping the same three shapes (completed/skipped/failed) plus the new
  `Partial`, so consumers gain a variant rather than a rewrite.
- **Event volume.** Recording every success-level event could grow the report;
  mitigated by keeping events source-grained (one per data source / processor /
  exporter batch class), not per document.
- **Silent-behavior change.** Failures that were invisible now surface in the feed and
  the exit code — expected and intended, but changes observed CLI exit status for runs
  that previously "passed" while dropping errors.
