## Why

ESDiag models diagnostic work four inconsistent ways — CLI subcommands, the runtime
`Collector`/`Processor` split, the persisted `Job { collect, action }`, and the web
`JobSignals` — and the shared verbs are overloaded across all of them. The runtime is a
strict subset of the space these encode: `handle_job_run` requires a collect host (no
`Load` input), every action collects to a bundle first (always staged, never streaming),
and `JobAction` fuses phases into three mutually-exclusive variants that cannot express
Save-and-Process together. We unify the backend on a single **`Job`** composed of six
stages within three phases, driven by **one executor**. Rationale: **ADR-0002** (the
six-stage model), **ADR-0003** (the name `Job`), **ADR-0004** (phase-structured `Job`
and stage-aligned modules).

## What Changes

- Introduce the phase-structured `Job { identifiers, input, save?, process?, send? }`,
  where `input` is `Collect` xor `Load` (Phase 1, required), `save` writes a raw bundle
  (Phase 2a), `process` transforms (Phase 2b), and `send` transmits a bundle (Phase 3).
- **`Export` lives inside `Process`** (Model β): `Process { selection, export }`. "Export
  ⟺ Process" becomes a type-level guarantee — "process to nowhere" and "export nothing"
  are unrepresentable. `Export` and `Send` are independent Phase-3 outputs and MAY both
  run in one job (Phase 3 is *and/or*, not xor).
- Enforce the dependency invariants at construction (the rest are unrepresentable):
  `save` ⟹ `input` is `Collect`; `send` ⟹ a bundle exists (`Load` or `save`); at least
  one of `save`/`process`/`send` is set.
- **Execution mode is derived, not stored:** `save` + `process` ⇒ *staged* (the bundle is
  a serialization barrier); `Collect` + `process` without `save` ⇒ *streaming* (receive,
  transform, and export overlap). One executor derives and drives both.
- New job shapes fall out for free: `Load`-input jobs, streaming jobs, and
  Save + Process + Export + Send in a single run — none expressible today.
- Included diagnostics execute as **child `Job`s** under the same executor, each minting a
  child `JobID`.
- **BREAKING (internal):** retire `Collector`, `Processor` (as distinct operation types),
  `JobAction`, the `JobCollect` fusion, and `into_collect_exporter`; converge the duplicate
  CLI-streaming and job-staged process paths onto the one executor. Realign modules to the
  stages (`job/`, `receiver/`, `processor/`, `exporter/`, `uploader.rs`).
- The web form binds the unified `Job` phases directly; `JobSignals` collapses to a thin
  presentation projection. UI verbs (collect/process/send) remain presentation labels.

## Capabilities

### New Capabilities

- _(none — this modifies existing capabilities)_

### Modified Capabilities

- `collection-execution`: replace the one-/two-job boundary with a single `Job` whose
  execution mode (staged vs streaming) is *derived* from `Save`; add the phase-composed
  `Job` model and its construction invariants, `Load` input, and concurrent Export + Send;
  converge collection and processing onto one executor.
- `diagnostic-workflow`: bind the web workflow to the unified `Job` phases (`JobSignals`
  becomes a projection); make Phase 3 *and/or* so a processed job MAY also forward its raw
  bundle in the same run.
- `included-diagnostic-jobs`: each included diagnostic executes as a child `Job` (a
  `Load`-input, processing job) under the one executor, minting a child `JobID`.

## Impact

- **Core:** new `job/` module (the `Job` model, phase types, validated construction, and
  the `executor` that derives staged vs streaming and drives the stages); `receiver/`
  (`Collect` + `Load` sources); `processor/` (transform only); `exporter/` split by role
  into `BundleExporter` (`Save`) + `DocumentExporter` (`Export`); `uploader.rs` (`Send`).
  Retires `Collector`, `Processor` types, `JobAction`, `JobCollect`, `into_collect_exporter`,
  and the duplicate CLI-streaming / job-staged paths.
- **CLI:** `collect` / `process` / `read` / `job run` build a `Job` and hand it to the one
  executor; the `collect --upload` handoff becomes a `Collect` + `save` + `send` job.
- **Web UI:** the form binds `Job` phases directly; `JobSignals` reduced to a projection;
  the `Send` panel can enable Export **and** Send together.
- **Out of scope:** the on-disk `jobs.yml` migration to the phase shape is owned by
  `saved-job-migration` (ADR-0009); this change owns the in-memory model and executor.
- **Depends on** `platform-application-split` (ADR-0001) for `Platform` propagation to
  child jobs.
