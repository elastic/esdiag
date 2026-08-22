# Tasks

## 1. Stage-aligned interfaces

- [x] 1.1 Add a Phase-1 input resolver that handles stable and runtime-bound
  `Collect`/`Load` references, including service-link materialization and nested
  child inputs.
- [x] 1.2 Split `exporter/` by role into `BundleExporter` (`Save`, raw) and
  `DocumentExporter` (`Export`, processed); make processed-to-bundle and
  raw-to-cluster unrepresentable.
- [x] 1.3 Expose `uploader.rs` as the `Send` adapter over a resolved bundle.
- [x] 1.4 Reduce the internal processing interface to transform plus its
  `DocumentExporter`; keep temporary compatibility adapters until callers move.

## 2. Deepen the executor

- [x] 2.1 Add runtime input/output binding references to ephemeral jobs and
  reject those references from saved-job persistence.
- [x] 2.2 Introduce `ExecutionContext` with input resolution, role-typed
  adapters, execution ID/owner, observer, retention policy, and child context.
- [x] 2.3 Replace the flag-only result with `ExecutionOutcome`: per-stage
  results, parent report/outcome, child outcomes, retained bundle, and upload
  result.
- [x] 2.4 Publish typed lifecycle/progress events through the context observer
  so CLI, web, and API callers project one execution.
- [x] 2.5 Materialize remote `Load` inputs when processing, raw Send, or retained
  download requires a bundle; keep `Save` restricted to `Collect`.
- [x] 2.6 Attempt Process/Export and raw-bundle Send independently after bundle
  resolution and aggregate their stage outcomes without hiding either success.

## 3. CLI convergence

- [x] 3.1 Route `collect` through the executor, including filters,
  `--sources`, `--save-job`, metadata, archive naming, and resolved emitted
  bundle path.
- [x] 3.2 Map `collect --upload` to one staged `Collect + Save + Send` job.
- [x] 3.3 Route every `process` input (archive, directory, known host, service
  link) through the executor.
- [x] 3.4 Preserve explicit output and `ESDIAG_OUTPUT_*` fallback, process
  `--save-job`, summaries/events, child links, and outcome-based exit status.
- [x] 3.5 Route standalone `upload` through a `Load(File) + Send` Job while
  preserving custom `--api-url`.
- [x] 3.6 Converge the duplicate CLI streaming and staged processing paths.

## 4. Web and synchronous API convergence

- [x] 4.1 Replace executable `JobSignals` with backend-owned `JobDraft` state
  that compiles to a validated `Job` plus runtime bindings.
- [x] 4.2 Give processed-document Export and raw-bundle Send separate draft
  targets and round-trip them without overwriting either value.
- [x] 4.3 Derive target availability on the backend from the draft, reject
  invalid combinations before execution, and patch form state/elements over
  SSE.
- [x] 4.4 Route async known-host, ad-hoc API-key, service-link, and uploaded
  archive jobs through the executor while preserving transient credentials,
  owner-scoped events, job caps/statistics, and retained downloads.
- [x] 4.5 Route synchronous `/api/api_key` and `/api/service_link` through the
  executor and preserve parent/child multi-result response arrays.

## 5. Included diagnostic child jobs

- [x] 5.1 Bind each included diagnostic path to a nested runtime Load input and
  execute a literal child `Job` (`Load + Process/Export`) through the executor.
- [x] 5.2 Mint a child `JobID`; inherit owner and parent `Platform`; preserve
  parent relationship metadata; reuse the parent's `DocumentExporter`.
- [x] 5.3 Carry child outcomes through the executor result/observer and enforce
  a depth-one fan-out limit.

## 6. Verification gates

- [x] 6.1 Cover every delta-spec scenario in `collection-execution`,
  `diagnostic-workflow`, `included-diagnostic-jobs`, `saved-jobs`, and
  `diagnostic-reporting`.
- [x] 6.2 Regression-test streaming overlap, bounded `document_channel`
  backpressure, and staged serialization.
- [x] 6.3 Test Process failure, Export failure after a completed report, and
  every Export/Send success/failure pair, including successful raw Send after
  Process failure and successful Export before Send failure.
- [x] 6.4 Test stable versus runtime bindings, saved-job rejection of runtime
  bindings, credential redaction, and one-use ad-hoc API keys.
- [x] 6.5 Run a CLI parity matrix for collect/process inputs, standalone
  upload, `collect --upload`, `--sources`, `--save-job`, environment output
  fallback, custom upload API URL, summaries, child links, and exit status.
- [x] 6.6 Run a web/API parity matrix for owner-scoped events, admission caps,
  statistics, retained downloads, service-mode output policy, synchronous
  result arrays, and simultaneous Export + Send.
- [x] 6.7 Test child jobs for nested input, distinct IDs, inherited owner and
  `Platform`, shared document export, preserved parent relation, all child
  outcomes, parent aggregate status, and one-level fan-out.

## 7. Retire compatibility paths

- [x] 7.1 Prove all verification gates pass and no production CLI, async web,
  synchronous API, saved-job, or child-processing call site constructs
  `Collector` or `Processor` directly.
- [x] 7.2 Remove `Collector` and `Processor` as operation types.
- [x] 7.3 Remove `into_collect_exporter` and temporary executor compatibility
  adapters.
- [x] 7.4 Remove the legacy two-job web handoff and then sync the
  one-/two-job requirement removal.

---

`JobAction` and `JobCollect` retirement is already complete through
`saved-job-migration`; `LegacyJobAction` and `LegacyJobCollect` remain only for
ADR-0009 `jobs.yml` migration and are outside this change.
