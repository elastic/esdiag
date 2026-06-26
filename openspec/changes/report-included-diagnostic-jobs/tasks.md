## 1. Processor Child Outcomes

- [ ] 1.1 Add an `IncludedDiagnosticOutcome` model that represents completed, skipped, and failed included diagnostic processing results.
- [ ] 1.2 Extend `Processor<Completed>` to expose included diagnostic outcomes while preserving the existing parent `report` field.
- [ ] 1.3 Update `spawn_sub_processors` so child processors return structured outcomes instead of logging and discarding completed reports.
- [ ] 1.4 Classify readable but unsupported child manifests as skipped informational outcomes without failing the parent processor.
- [ ] 1.5 Classify child processing failures as failed child outcomes without failing the parent processor when parent processing succeeds.
- [ ] 1.6 Preserve child report parent metadata and orchestration identifiers when processing children from ECK or KubernetesPlatform parents.
- [ ] 1.7 Ensure included diagnostics use default child product processing selection instead of parent-level processor filters.

## 2. Web Job Events and Templates

- [ ] 2.1 Add child diagnostic job event data for queued/started/completed/skipped/failed included diagnostics.
- [ ] 2.2 Update `run_processor_job` to emit a separate progress box for each started included diagnostic child job.
- [ ] 2.3 Add or update Askama job templates for child completion results using each child report's `diagnostic.id`, Kibana link, product, document count, and duration.
- [ ] 2.4 Add an informational skipped-child template/status for recognized unsupported included diagnostics.
- [ ] 2.5 Ensure ECK/KubernetesPlatform parent completion does not render as the only successful diagnostic result when child outcomes exist.

## 3. CLI Process Summary

- [ ] 3.1 Update CLI process summary formatting to consume the completed processor state, including included diagnostic outcomes.
- [ ] 3.2 Print each completed child diagnostic's product, document count, `diagnostic.id`, and Kibana link when present.
- [ ] 3.3 Print recognized unsupported included diagnostics as informational skipped entries.
- [ ] 3.4 Print failed child diagnostics as failed child entries without changing successful parent output to a command failure.
- [ ] 3.5 Preserve the existing single-report CLI summary for normal non-parent diagnostic processing.

## 4. Synchronous API Results

- [ ] 4.1 Update synchronous `/api/api_key` processing to return a JSON array of diagnostic result entries.
- [ ] 4.2 Update synchronous `/api/service_link` processing to return the same JSON array shape as `/api/api_key`.
- [ ] 4.3 Include the parent diagnostic entry and each included diagnostic outcome entry in API arrays.
- [ ] 4.4 Represent skipped included diagnostics as `status: "info"` API entries with a reason.
- [ ] 4.5 Represent failed included diagnostics as `status: "failed"` API entries without failing the response when parent processing succeeds.

## 5. Tests

- [ ] 5.1 Add processor tests for a parent manifest with multiple supported included diagnostics returning multiple completed child outcomes.
- [ ] 5.2 Add processor tests for readable unsupported included diagnostics returning skipped informational outcomes.
- [ ] 5.3 Add processor tests for child failures returning failed child outcomes while the parent processor completes.
- [ ] 5.4 Add job runner or template tests that verify one UI result per supported child diagnostic with distinct diagnostic ids and Kibana links.
- [ ] 5.5 Add UI/template tests for unsupported child diagnostics rendering an `info` skipped result.
- [ ] 5.6 Add CLI summary tests that verify child diagnostic ids, child Kibana links, skipped unsupported children, and failed child entries are included for parent bundles.
- [ ] 5.7 Add synchronous API tests that verify parent, successful child, skipped child, and failed child entries are returned in JSON arrays.
- [ ] 5.8 Add regression coverage for normal single-diagnostic processing to confirm existing parent `report` behavior remains unchanged.

## 6. Documentation and Verification

- [ ] 6.1 Update nearby CLI, API, web, or processing documentation if implementation details change user-visible behavior.
- [ ] 6.2 Update `CHANGELOG.md` with the improved ECK/KubernetesPlatform included diagnostic reporting.
- [ ] 6.3 Run `cargo fmt`.
- [ ] 6.4 Run `cargo clippy`.
- [ ] 6.5 Run `cargo test`.
