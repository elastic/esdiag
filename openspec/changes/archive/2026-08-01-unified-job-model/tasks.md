# Tasks

## 1. Job model (`job/`)
- [x] 1.1 Define `Job { identifiers, input, save?, process?, send? }` with `Input` (`Collect | Load`), `SaveTarget`, `Process { selection, export }`, `SendTarget`, and `ExportTarget`.
- [x] 1.2 Fuse `Export` into `Process` (Model β) so "process to nowhere" and "export nothing" are unrepresentable.
- [x] 1.3 Add a validated constructor enforcing the invariants: `save` ⟹ `Collect`; `send` ⟹ bundle exists (`Load` or `save`); at least one of `save`/`process`/`send`. Return typed construction errors.
- [x] 1.4 Derive execution mode (staged vs streaming) from the stage selection; expose it to the executor. Do not store it.

## 2. Executor (`job/executor`)
- [x] 2.1 Implement one executor that derives the mode and drives the stages for both staged and streaming jobs.
- [x] 2.2 Staged path: run `Collect`, materialise the bundle (serialization barrier), then `Process` reads the bundle.
- [x] 2.3 Streaming path: overlap receive, transform, and export using the existing `get_stream` / `StreamingDataSource` / `document_channel` machinery.
- [x] 2.4 Compose Phase 3 as *and/or*: run `Export` (inside `Process`) and/or `Send` in one run.

## 4. Retire legacy types and paths
- [x] 4.2 Remove `JobAction` and the `JobCollect` fusion; construct `Job`s from phases.

## 7. Verification
- [x] 7.1 Unit tests for the constructor invariants (each violation rejected; each valid shape accepted).
- [x] 7.2 Test derived mode: `Save`+`Process` ⇒ staged; `Collect`+`Process` without `Save` ⇒ streaming.
- [x] 7.3 Test `Load`-input jobs (load→process, load→send) and Save+Process+Export+Send in one run.
- [x] 7.6a Confirm the delta spec scenarios in `specs/collection-execution` are covered.

---

## Scope note (2026-08-01)

This change landed per the design's phased strategy — "land the `job/` executor behind the
existing surfaces first, then remove the old paths once both drive the executor" — and its
scope is now **the first half only**: the phase-structured `Job` model with validated
construction and typed errors (1.x), the derived execution mode, and the one executor (2.x)
driving staged (Collect→Save barrier→Process), streaming (Collect+Process, no Save),
`Load`-input, and and/or Phase-3 (Export+Send) shapes, with `esdiag job run` (CLI and saved
jobs) running through it.

4.2 completed via `saved-job-migration`: the persisted job *is* the phase model
(`data::Job` re-exports `job::model::Job`), so there is no longer a legacy shape to convert
at the `job run` boundary and `JobBuilder` builds through `Job::try_new`. `LegacyJobAction`
and `LegacyJobCollect` survive only to migrate an existing `jobs.yml` (ADR-0009).

The second half — routing `collect` / `process` / `read` and the web `job_runner` through
the executor, the stage-aligned module split, retiring `Collector` / `Processor` /
`into_collect_exporter`, child jobs as literal `model::Job`s, and the one-/two-job removal —
moved to the **`executor-convergence`** change, tracked by #368. Its delta specs
(`diagnostic-workflow`, `included-diagnostic-jobs`, and the `collection-execution` removal)
moved with it, so what remains here describes only shipped behavior.
