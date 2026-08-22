# Design

The six stages and `Job` name come from ADR-0002/0003. ADR-0004 settles
Model beta: Export is structurally part of Process, while Export and raw-bundle
Send may both be selected. `unified-job-model` implemented the serializable
phase model and a first executor for saved jobs. This change deepens that
executor interface so all runtime surfaces can use it.

## Context

The current `execute(Job) -> JobOutcome` interface assumes it can resolve
saved hosts and exporters from process-global configuration. It discards the
completed processor report and exposes only bundle/upload flags. Other callers
have requirements that do not fit that interface:

- web API-key collection carries a one-use in-memory credential that must not
  enter `hosts.yml` or a serialized `Job`;
- service mode supplies its output exporter at startup and must not use
  per-user host persistence;
- CLI `process` may resolve output from `ESDIAG_OUTPUT_*` and must preserve
  reports, child links, and outcome-based exit status;
- CLI `upload` supplies a custom upload API base URL that belongs to the runtime
  sender adapter rather than persisted `SendTarget`;
- the async web runner publishes owner-scoped progress, updates statistics, and
  manages retained browser downloads;
- synchronous APIs return parent and child result arrays;
- included diagnostics are nested receiver paths, not standalone filesystem
  URIs.

The UI also necessarily holds incomplete state while a user edits it. A valid
`Job` rejects empty hosts, missing outputs, and no-work combinations, so form
signals cannot themselves be an executable `Job`.

## Decisions

### Keep declaration, runtime resources, and UI drafts separate

`Job` remains the serializable, validated declaration of selected phases.
Stable references (saved-host names, filesystem paths, service links) remain in
the declaration. A runtime-only input or output is represented by an opaque
binding key; credentials and adapters live only in the execution context.
Saved-job validation SHALL reject runtime-only bindings, while an ephemeral
CLI/web/API job MAY use them for the duration of one execution.

`JobDraft` is the backend-owned editable representation of the web form.
Datastar signals carry only interaction/form values. The backend validates and
patches target availability, then compiles the draft into a valid `Job` and
the runtime bindings needed by its `ExecutionContext`. The draft has separate
fields for:

- the Phase-1 input choice and its transient form data;
- retained-bundle choice;
- Process and its processed-document Export target;
- raw-bundle Send and its upload-service target.

This permits incomplete form state and simultaneous Export + Send without
weakening `Job` construction invariants.

### Deepen the executor seam

The executor interface becomes conceptually:

```text
execute(job: Job, context: ExecutionContext) -> ExecutionOutcome
```

`ExecutionContext` supplies execution-only capabilities:

- resolution of stable and runtime-bound Collect/Load inputs;
- role-typed `BundleExporter` and `DocumentExporter` adapters;
- raw-bundle sender;
- execution identity (`JobID`, owner) and progress observer;
- retained-input/bundle publication policy;
- optional parent bundle, inherited `Platform`, and child depth.

The default CLI/saved-job context resolves stable references from existing
configuration. Web and synchronous API contexts bind transient hosts,
credentials, startup exporters, owner-scoped observers, and retained-download
publication without storing those resources in `Job`.

Input resolution yields a receiver and, when present or materialized, a bundle
handle. A service link is a `Load` input; the resolver downloads it when a
downstream stage or retained-download policy needs a local bundle. `Save`
continues to mean serializing a newly collected diagnostic and therefore still
requires `Collect`. Retaining an already-loaded service-link/upload bundle is
execution policy over the resolved input, not a second Save stage.

### Return one structured execution result

`ExecutionOutcome` is the caller-facing result of all selected stages. It
contains:

- per-stage completion/failure;
- parent diagnostic report and derived diagnostic outcome when Process ran;
- child diagnostic outcomes;
- retained bundle path/handle when applicable;
- upload result when Send ran.

The observer reports queued/started/progress/completed stage events with
execution identity. CLI summaries, web feed events/statistics, and synchronous
API arrays are projections of the same outcome/events; they do not run
alternative processing paths.

An input-resolution or Collect failure prevents all dependent stages. Once a
bundle exists, Process/Export and raw-bundle Send are independent selected
outputs: the executor attempts both, preserves either success, and records both
failures. Process and Export have separate stage results even though Export is
structurally owned by Process: a Process failure may produce no completed
report, while an Export hard failure after transformation preserves any report
already produced. Exporter transport/document failures recorded in that report
continue to affect its derived `DiagnosticOutcome`; the executor never assigns
a competing verdict. A caller returns a non-success terminal status when any
selected parent stage hard-fails, while still presenting successful stage
results.

`DiagnosticOutcome` remains authoritative for the diagnostic report's verdict;
`ExecutionOutcome` is authoritative for the whole job's terminal status. A
successful diagnostic with a failed Send therefore keeps its successful
diagnostic verdict but has a non-success job status.

Streaming `Collect + Process/Export` has no bundle and therefore cannot select
raw-bundle Send. A live Collect that selects Send must also select Save
(temporary or retained), making the job staged.

### Model included diagnostics as child executions

A child is an ephemeral `Job` with `Load + Process/Export`. Its runtime Load
binding identifies a nested diagnostic path relative to the parent's resolved
bundle; it does not pretend that path is a standalone URI. The child context:

- mints a distinct child `JobID`;
- inherits owner and parent `Platform`;
- reuses the parent's `DocumentExporter` adapter;
- records the parent relationship;
- increments depth and rejects fan-out when depth is already one.

Child execution outcomes flow through the same observer/outcome contract and
preserve any separately derived child `DiagnosticOutcome`. The parent collects
completed, partial, failed, and skipped child execution results without turning
a child failure into loss of the parent result. Child failures remain child
outcomes and do not fail the parent's Process stage or aggregate terminal
status; synchronous APIs therefore retain their existing HTTP 200 response
when parent processing succeeds.

### Converge every execution surface

The convergence includes:

1. CLI `collect`, `process`, and standalone `upload`, including `--sources`,
   `--save-job`, environment output fallback, `collect --upload`, custom upload
   API URL, summaries, child links, and exit status.
2. The asynchronous web runner for known hosts, ad-hoc API keys, service links,
   and uploaded archives.
3. Synchronous `/api/api_key` and `/api/service_link` processing and their
   multi-result JSON responses.
4. Included diagnostic child processing.

There is no `read` CLI command; existing archive, directory, known-host, and
service-link inputs remain forms of `process` input.

## Implementation order

1. Introduce role-typed stage adapters and the input-resolution seam.
2. Add `ExecutionContext`, structured outcomes/events, runtime bindings, and
   independent output completion while retaining compatibility adapters for
   legacy callers.
3. Converge CLI surfaces and verify behavior parity.
4. Introduce `JobDraft`, converge async web execution, then converge synchronous
   APIs.
5. Convert included diagnostics to child jobs.
6. Prove no production caller constructs the legacy operation types, then
   remove them and the compatibility adapters.
7. Remove the one-/two-job requirement only after web convergence.

## Rejected alternatives

- **Put resolved hosts, credentials, server state, or exporters in `Job`.**
  Rejected because it makes persistence unsafe and couples the domain model to
  one runtime.
- **Treat form signals as a partially valid `Job`.** Rejected because it
  weakens construction invariants and still cannot represent two independent
  output targets cleanly.
- **Let each caller wrap the executor and reconstruct reports/events.**
  Rejected because it preserves the duplicate execution paths this change is
  intended to remove.
- **Represent nested diagnostics as fake filesystem paths.** Rejected because
  archive-backed receivers do not expose children as standalone paths.

## Risks

- **Wide blast radius.** Compatibility adapters and surface-by-surface parity
  tests contain the migration.
- **Streaming/backpressure regression.** Bounded `document_channel` behavior
  and overlapping receive/transform/export must be measured before retirement.
- **Credential leakage.** Runtime bindings must be non-serializable through the
  saved-job interface and redacted in logs/events.
- **Event/reporting drift.** All existing owner, statistics, retained-download,
  CLI summary, and synchronous API contracts need explicit parity coverage.
- **Partial-output ambiguity.** Stage-level outcomes and aggregate terminal
  status are specified and tested for every Export/Send success/failure pair.
