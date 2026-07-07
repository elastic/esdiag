# Tasks

## 1. Job model (`job/`)
- [ ] 1.1 Define `Job { identifiers, input, save?, process?, send? }` with `Input` (`Collect | Load`), `SaveTarget`, `Process { selection, export }`, `SendTarget`, and `ExportTarget`.
- [ ] 1.2 Fuse `Export` into `Process` (Model β) so "process to nowhere" and "export nothing" are unrepresentable.
- [ ] 1.3 Add a validated constructor enforcing the invariants: `save` ⟹ `Collect`; `send` ⟹ bundle exists (`Load` or `save`); at least one of `save`/`process`/`send`. Return typed construction errors.
- [ ] 1.4 Derive execution mode (staged vs streaming) from the stage selection; expose it to the executor. Do not store it.

## 2. Executor (`job/executor`)
- [ ] 2.1 Implement one executor that derives the mode and drives the stages for both staged and streaming jobs.
- [ ] 2.2 Staged path: run `Collect`, materialise the bundle (serialization barrier), then `Process` reads the bundle.
- [ ] 2.3 Streaming path: overlap receive, transform, and export using the existing `get_stream` / `StreamingDataSource` / `document_channel` machinery.
- [ ] 2.4 Compose Phase 3 as *and/or*: run `Export` (inside `Process`) and/or `Send` in one run.

## 3. Stage-aligned modules
- [ ] 3.1 `receiver/` — resolve Phase-1 input uniformly for `Collect` (remote, client) and `Load` (local/download, no client).
- [ ] 3.2 `processor/` — reduce to transform-only per-API processors; remove collection/sink orchestration.
- [ ] 3.3 `exporter/` — split by role into `BundleExporter` (`Save`, raw) and `DocumentExporter` (`Export`, processed); make processed-to-bundle and raw-to-cluster unrepresentable.
- [ ] 3.4 `uploader.rs` — the `Send` stage over an existing bundle.

## 4. Retire legacy types and paths
- [ ] 4.1 Remove `Collector` and `Processor` as distinct operation types; route both through the executor.
- [ ] 4.2 Remove `JobAction` and the `JobCollect` fusion; construct `Job`s from phases.
- [ ] 4.3 Remove `into_collect_exporter`.
- [ ] 4.4 Converge the duplicate CLI-streaming and job-staged process paths onto the one executor.

## 5. CLI and Web surfaces
- [ ] 5.1 CLI `collect` / `process` / `read` / `job run` build a `Job` and hand it to the executor; map `collect --upload` to a `Collect` + `save` + `send` job.
- [ ] 5.2 Bind the web form to the `Job` phases; collapse `JobSignals` to a thin projection.
- [ ] 5.3 `Send` panel: derive target availability from active phases; allow processed-output and raw-bundle targets to be enabled together when a bundle is retained.

## 6. Child jobs
- [ ] 6.1 Spawn each included diagnostic as a child `Job` (`Load` input + `Process`) under the one executor, minting a child `JobID`.
- [ ] 6.2 Set each child job's `Platform` from the parent as it spawns; keep fan-out one level deep.

## 7. Verification
- [ ] 7.1 Unit tests for the constructor invariants (each violation rejected; each valid shape accepted).
- [ ] 7.2 Test derived mode: `Save`+`Process` ⇒ staged; `Collect`+`Process` without `Save` ⇒ streaming.
- [ ] 7.3 Test `Load`-input jobs (load→process, load→send) and Save+Process+Export+Send in one run.
- [ ] 7.4 Regression: streaming concurrency/backpressure preserved after path convergence.
- [ ] 7.5 Test child diagnostics execute as child jobs with inherited `Platform`, one level deep.
- [ ] 7.6 Confirm the delta spec scenarios in `specs/collection-execution`, `specs/diagnostic-workflow`, and `specs/included-diagnostic-jobs` are covered.
