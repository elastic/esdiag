## Why

Success is never defined: a diagnostic report carries only counts (`docs`, `errors`)
with no verdict, and the pipeline degrades silently — `ProcessorSummary::merge(Err)`
drops a whole-source failure to a `tracing::warn!`, so "what failed and why" survives
only in logs. Children already have an outcome; the parent does not. Rationale:
**ADR-0016**.

## What Changes

- Introduce a first-class `DiagnosticOutcome` (`Complete | Partial | Failed |
  Skipped`) that applies to **any** diagnostic — parent or child. The outcome is
  **derived** from the report's recorded events: any collected failure/partial
  capture → `Partial`; total failure → `Failed`; unsupported → `Skipped`; all good →
  `Complete`.
- **BREAKING (internal):** the child-only `IncludedDiagnosticOutcome` unifies into the
  single `DiagnosticOutcome`, and the top-level counts-only model gains a verdict.
- The diagnostic report **records all error/warning/success-level events** (each with
  source + reason) rather than only aggregate counts — failures are *collected*, not
  logged-and-dropped. `ProcessorSummary::merge(Err)` records a failure event instead
  of warning; collection **and** processing failures persist.
- The report becomes the **persisted source of truth**: the owner-scoped job feed
  (ADR-0008) renders failures from it, and the CLI exit code and WebUI status read the
  same single outcome.
- Make export status **two-level**: a request/transport code (a `_bulk` request may be
  `200`) is distinct from document-level codes (per-doc `409`/`429`). The per-doc
  `status_counts` histogram is authoritative for document outcomes and feeds the
  `Partial` verdict; **HTTP `0` is reserved** for non-HTTP exporters (file, stream,
  directory) and never used to mean "mixed".
- `Skipped` distinguishes **by-design** (out of scope, e.g. platform-level API
  collection) from **not-implemented** (work-in-progress, e.g. Kibana processing), per
  **ADR-0019**.

## Capabilities

### New Capabilities

- _(none — this modifies existing capabilities)_

### Modified Capabilities

- `diagnostic-reporting`: add the derived `DiagnosticOutcome`; record all
  error/warning/success events with source + reason (collect, don't drop); make the
  report the persisted source of truth for the job feed, CLI exit code, and WebUI
  status; define two-level export status with an authoritative `status_counts`
  histogram and reserved HTTP `0`; distinguish `Skipped` by-design from
  not-implemented.
- `included-diagnostic-jobs`: retype the preserved child outcome from the child-only
  `IncludedDiagnosticOutcome` to the unified `DiagnosticOutcome`, so a child can be
  `Partial` and a skip carries its by-design/not-implemented reason.

## Impact

- **Core processing:** `DiagnosticReport` / `DiagnosticReportBuilder` and
  `ProcessorSummary::merge`/`add_child` (`src/processor/diagnostic/report.rs`) stop
  warn-and-dropping and record events; `IncludedDiagnosticOutcome` collapses into
  `DiagnosticOutcome` (`src/processor/mod.rs`); exporters
  (`src/exporter/{elasticsearch,file,stream,directory}.rs`) set request vs document
  status and reserve HTTP `0`.
- **CLI:** the `process` exit code reads the single outcome; summaries print the
  recorded events (source + reason) instead of relying on tracing logs.
- **Web UI:** the job-feed status and per-source rows render from the persisted report;
  child rows show a unified outcome, including `Partial`.
- **Depends on** `platform-application-split` (for the typed child metadata); part of
  the architecture-review series.
