## Why

`unified-job-model` (ADR-0002, ADR-0003, ADR-0004) landed the phase-structured
`Job` and the first executor behind the existing surfaces. Only `esdiag job run`
uses that executor today. `esdiag collect` and `process`, the asynchronous web
runner, and the synchronous `/api/api_key` and `/api/service_link` handlers still
construct `Collector` or `Processor` directly.

The first executor also resolves saved hosts and exporters itself and returns
only bundle/upload flags. That interface is sufficient for saved jobs, but not
for transient web credentials, service-mode exporters, owner-scoped events,
CLI reports, synchronous API result arrays, or included diagnostics nested
inside a parent bundle.

This change completes #368 by deepening the executor interface and then routing
every execution surface through it. The serializable `Job` remains the
phase-structured description of work; execution-only resources and reporting
move through an `ExecutionContext` and `ExecutionOutcome`.

## What Changes

- Realign modules to the stages: `receiver/` resolves Phase-1 `Collect` and
  `Load` inputs; `processor/` becomes transform-only; `exporter/` splits into
  role-typed `BundleExporter` (`Save`, raw) and `DocumentExporter` (`Export`,
  processed); `uploader.rs` remains `Send`.
- Deepen the executor to accept an `ExecutionContext` that resolves stable job
  references and supplies transient inputs/outputs, owner, progress observer,
  bundle retention, and child-execution context without persisting credentials
  or server state.
- Return a structured `ExecutionOutcome` containing stage results, the parent
  diagnostic report, child outcomes, retained bundle information, and upload
  result. Callers render CLI summaries, web events, statistics, and synchronous
  API responses from that one result/event stream.
- Treat Export and raw-bundle Send as independent selected outputs once a bundle
  exists. Attempt both even when one fails and aggregate their stage outcomes;
  an input failure still prevents downstream stages.
- Route CLI `collect`, `process`, and standalone `upload`, the asynchronous web
  runner, and synchronous API handlers through the executor.
  `collect --upload` becomes a
  `Collect + Save + Send` job using the actual emitted archive path.
- Replace executable `JobSignals` state with an editable `JobDraft`. The draft
  mirrors phase choices, keeps processed-output and raw-bundle targets
  separately, and compiles on the backend into a validated `Job` plus execution
  context. Incomplete form state is never treated as an executable `Job`.
- Execute included diagnostics as literal child `Job`s. A child context binds
  its nested path to the parent resolved bundle and carries child ID, owner,
  inherited platform, shared document exporter, and depth. Fan-out remains one
  level deep.
- Retire `Collector`, `Processor` as operation types, `into_collect_exporter`,
  and the duplicate CLI/web execution paths only after every caller converges.
- Remove the legacy one-/two-job web workflow requirement after web convergence.

## Capabilities

### New Capabilities

- _(none — this modifies existing capabilities)_

### Modified Capabilities

- `collection-execution`: define runtime resolution and structured executor
  results, independent Export/Send completion, surface convergence, and removal
  of the one-/two-job boundary.
- `diagnostic-workflow`: compile a backend-owned editable draft into the unified
  phases and support independent processed-output and raw-bundle targets.
- `included-diagnostic-jobs`: execute each included diagnostic as a nested-input
  child `Job` under the same executor with inherited execution context.
- `saved-jobs`: distinguish the stable-reference subset that may be persisted
  from ephemeral Jobs containing runtime bindings, and project saved jobs into
  `JobDraft` for editing.
- `diagnostic-reporting`: keep `DiagnosticOutcome` authoritative for the
  diagnostic verdict while `ExecutionOutcome` is authoritative for whole-job
  terminal status.

## Impact

- **Core:** changes the executor interface and result contract; restructures
  `receiver/`, `processor/`, `exporter/`, and `uploader.rs`; then removes the
  legacy operation types.
- **CLI:** `main.rs` routes `collect`, `process`, and `upload` through the
  executor while preserving `--sources`, `--save-job`, environment output
  fallback, custom upload API URL, summaries, child links, and exit behavior.
- **Web/API:** the asynchronous runner and synchronous APIs use the same
  executor while preserving transient credential custody, service-mode output
  policy, owner-scoped events, retained downloads, statistics, and result
  arrays.
- **Risk:** wide and regression-prone. Streaming concurrency/backpressure,
  credential non-persistence, event ownership, and partial-output reporting
  require explicit parity coverage before legacy retirement.
- **Depends on:** the archived `unified-job-model` implementation on the
  `architecture-review` branch.
