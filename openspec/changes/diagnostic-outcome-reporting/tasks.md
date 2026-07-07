# Tasks

## 1. Outcome type
- [ ] 1.1 Add `DiagnosticOutcome` (`Complete | Partial | Failed | Skipped`) with serde/`Display`; model `Skipped` to carry a by-design vs not-implemented discriminator (per ADR-0019).
- [ ] 1.2 Implement derivation from recorded report events (failure/partial → `Partial`; total failure → `Failed`; unsupported → `Skipped`; else `Complete`) as the only way to obtain an outcome.
- [ ] 1.3 Expose the derived `DiagnosticOutcome` on `DiagnosticReport` (`src/processor/diagnostic/report.rs`).

## 2. Event recording
- [ ] 2.1 Add an event log to the report — each event carries severity (`error | warning | success`), source, and reason.
- [ ] 2.2 Change `ProcessorSummary::merge(Err)` and `add_child(Err)` (`report.rs:487`, `:501`) to record a failure event instead of `tracing::warn!`.
- [ ] 2.3 Record collection failures as events (source + reason), not only logs.
- [ ] 2.4 Record success-level events for collected/processed sources.

## 3. Two-level export status
- [ ] 3.1 Keep the scalar request `status_code` as the transport verdict; make `status_counts` authoritative for document outcomes and feed the `Partial` derivation.
- [ ] 3.2 Fix `BatchResponse::merge` (`report.rs:420`) so mixed HTTP request codes are not collapsed to `0`.
- [ ] 3.3 Reserve request status `0` for non-HTTP exporters only (`src/exporter/{file,stream,directory}.rs`); ensure the Elasticsearch exporter never emits `0`.

## 4. Unify child outcome
- [ ] 4.1 Replace `IncludedDiagnosticOutcome` (`src/processor/mod.rs:101`) with `DiagnosticOutcome`; derive each child's outcome from its own child report.
- [ ] 4.2 Update the fan-out (`spawn_sub_processors`) and completed-state accessors to carry the unified outcome, including `Partial`.

## 5. Consumers read the report
- [ ] 5.1 CLI `process` exit code derived from the report's `DiagnosticOutcome`; print recorded events (source + reason) in the summary.
- [ ] 5.2 WebUI status and the owner-scoped job feed render from the persisted report's outcome and events (parent and child rows, incl. `Partial`).
- [ ] 5.3 Map the synchronous API result entries to the unified outcome (`src/server/api.rs`).

## 6. Verification
- [ ] 6.1 Unit tests for outcome derivation (all-success → `Complete`; one failure → `Partial`; total failure → `Failed`; unsupported → `Skipped` by-design vs not-implemented).
- [ ] 6.2 Test that a merged `Err` produces a recorded failure event, not a dropped log.
- [ ] 6.3 Test two-level status: `_bulk` `200` with per-doc `409` → `Partial`; mixed HTTP codes not collapsed to `0`; non-HTTP exporter reports `0`.
- [ ] 6.4 Test a child derives `Partial`/`Failed`/`Skipped` while the parent still completes.
- [ ] 6.5 Confirm the delta spec scenarios in `specs/diagnostic-reporting/spec.md` and `specs/included-diagnostic-jobs/spec.md` are covered.
