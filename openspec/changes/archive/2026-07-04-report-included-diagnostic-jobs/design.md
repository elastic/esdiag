## Context

ECK and KubernetesPlatform diagnostics are orchestration-level bundles. Their manifest can include `included_diagnostics`, and the current processor already fans those paths out into child processors from `Processor<Ready>::start`. The parent ECK/KubernetesPlatform processor intentionally does not process orchestration metrics today; it only validates connectivity and logs that included Elasticsearch bundles are processed elsewhere.

The current fan-out is invisible to callers. `spawn_sub_processors` starts child `Processor` tasks, but each task only logs completion or failure. `Processor<Completed>` exposes a single `DiagnosticReport`, so `src/server/job_runner.rs` can only render one `JobCompleted` template and `src/main.rs` can only print one CLI process summary using the parent report. For ECK/KubernetesPlatform parents that means both UI and CLI output can show an empty parent diagnostic id and Kibana link while hiding the useful Elasticsearch child reports.

## Goals / Non-Goals

**Goals:**
- Preserve the existing `Processor<Ready> -> Processor<Processing> -> Processor<Completed>` lifecycle while exposing structured child outcomes from `included_diagnostics`.
- Allow the web job runner to render one progress/result box per included child diagnostic started from a parent bundle.
- Allow the CLI `process` command summary to print each processed child diagnostic id and Kibana link.
- Return synchronous API processing results as a JSON array with one entry for the parent diagnostic and one entry for each included diagnostic outcome.
- Render completed child diagnostics with each child report's `diagnostic.id`, product, ingest counts, duration, and Kibana link.
- Render recognized but unsupported included diagnostics as informational skipped results.
- Keep parent/child metadata behavior aligned with the existing `orchestration-metadata` capability, including `parent_id` and orchestration identifiers.
- Preserve the parent diagnostic report and result entry even when the parent currently has no processed orchestration-level documents.

**Non-Goals:**
- Processing orchestration-level ECK or KubernetesPlatform metrics.
- Implementing new child diagnostic processors such as Kibana processing.
- Per-product child processor filtering or UI selection for included diagnostics.
- Recursive reporting for multi-level included diagnostic hierarchies.
- Changing the diagnostic report document schema written to Elasticsearch.

## Decisions

### Add structured child outcomes to the processor lifecycle

`Processor<Completed>` will retain the existing `report: DiagnosticReport` field and add a child outcome collection, for example `included_diagnostics: Vec<IncludedDiagnosticOutcome>`. This preserves existing callers that read `completed.state.report` while giving web orchestration a way to inspect child results.

`IncludedDiagnosticOutcome` will represent:
- `Completed`: child source/path, product, `DiagnosticReport`, runtime, and derived Kibana link from the child report.
- `Skipped`: child source/path, detected or declared product/type when available, and an informational reason.
- `Failed`: child source/path and error string.

Child outcomes are independent from parent success. A failed child becomes a failed child outcome but does not cause the parent processor to fail if parent processing itself succeeds. A parent with no children, all children skipped, or a mix of completed/skipped/failed children can still complete successfully as a parent result.

Alternative considered: replace the completed state with a single aggregate report. That would force callers to understand a new aggregate report shape and would still not map cleanly to per-child UI boxes.

### Use a processor event sink for child job progress

The processor will support an optional child-event sink used by the web job runner. During `Ready -> Processing`, each included diagnostic will produce a child job event before work starts. Supported child processors will emit `Started` and then `Completed` or `Failed`; unsupported children will emit `Skipped` with an `info` status. The default constructor path will use no event sink, so CLI and API callers will read completed child outcomes after processing instead of receiving live progress callbacks.

Alternative considered: emit all child UI boxes only after the parent processor completes. That would fix missing Kibana links but would not satisfy the user experience requirement for a separate progress box for each child diagnostic job as it starts.

### Classify unsupported children without failing the parent

Unsupported `included_diagnostics` will be detected when a child receiver can be cloned and its manifest can be read, but `Diagnostic::try_new` returns an unsupported-product or unimplemented-processor error. Those children will become `Skipped` outcomes with an informational status. Receiver clone/read failures and runtime processing failures become `Failed` child outcomes, but do not fail the parent processor.

Alternative considered: continue logging unsupported children as errors. That hides useful context from the UI and makes intentionally skipped child diagnostics look like unexpected failures.

### Keep parent completion separate from child completion in the UI

For a parent ECK/KubernetesPlatform bundle with included diagnostics, the web UI will preserve the parent result and also render child result boxes. The parent is valuable for context and future orchestration processing work, but it must not be the only visible result when child outcomes exist. Actionable child Kibana links will come from child outcomes.

Alternative considered: mutate the parent report to point at the first child diagnostic. That would be inaccurate metadata and would lose multi-child bundles.

### Extend human-readable CLI summary formatting from completed processor state

The CLI `process` command will keep human-readable output and format the full `Processor<Completed>` state, not only `Completed.report`. For normal single-diagnostic processing, output remains the existing single summary. For parent bundles with included diagnostic outcomes, the summary will list the parent result plus each completed child diagnostic with product, document count, `diagnostic.id`, and Kibana link when available. Recognized unsupported children will be listed as informational skipped entries, and child failures will be listed as failed child entries without changing the parent success result.

Alternative considered: print child links from sub-processor logs. Logs are not stable command output and do not give callers a predictable summary to copy into support workflows.

### Return synchronous API results as arrays

Synchronous processing endpoints will return an array of diagnostic result entries instead of a single result object. The array will include the parent diagnostic entry and one entry for each included diagnostic outcome. Completed entries include `status: "success"`, `diagnostic_id`, `kibana_link`, `took`, `product`, and source/path context. Skipped entries include `status: "info"` and a reason. Failed child entries include `status: "failed"` and an error string. A parent processing failure still returns the endpoint's existing error response because no completed parent result exists.

The service-link spec already requires `/api/service_link?wait_for_completion=true` to match synchronous `/api/api_key` behavior, so this change modifies that response shape contract.

Alternative considered: keep the existing single object and add a nested `included_diagnostics` field. A top-level array is more direct for consumers that need one entry for each diagnostic.

### Use default child processing selection

Included diagnostics will process with their default product behavior. This change will not add UI or CLI filtering for child processors because children can have different products and per-product selection needs a separate design.

### Keep included diagnostics flat

The expected bundle shape is one parent level with direct `included_diagnostics`. The implementation will not recursively flatten multi-level child hierarchies for this change.

## Risks / Trade-offs

- Child tasks run concurrently and completion order may vary -> accepted; neither UI nor CLI requires manifest-order reporting.
- Existing callers may ignore the new child outcomes -> preserve `Completed.report` unchanged and add tests for legacy single-diagnostic processing.
- CLI summary output becomes longer for orchestration parent bundles -> only expand output when child outcomes are present.
- Unsupported-child detection may initially rely on broad error strings -> prefer an internal enum or typed error classification when touching `Diagnostic::try_new`.
- Successful parent jobs with all children skipped may still have no ingested documents -> accepted; render the parent success and skipped children explicitly so users understand what happened.
- Changing synchronous API responses from object to array is a compatibility change -> document the new response shape and update tests around both `/api/api_key` and `/api/service_link`.
