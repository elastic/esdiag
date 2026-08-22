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
- Retire `JobAction` and the `JobCollect` fusion; `Job`s are constructed from phases.

Per the design's phased strategy the executor lands **behind** the existing surfaces:
`esdiag job run` drives it, while `collect` / `process` / `read` and the web `job_runner`
keep their current paths until the follow-up retires them.

## Capabilities

### New Capabilities

- _(none — this modifies existing capabilities)_

### Modified Capabilities

- `collection-execution`: add the phase-composed `Job` model and its construction
  invariants, the derived staged-vs-streaming execution mode driven by one executor,
  `Load` input, and concurrent Export + Send.

## Impact

- **Core:** new `job/` module — the `Job` model, phase types, validated construction, and
  the `executor` that derives staged vs streaming and drives the stages. Retires
  `JobAction` and the `JobCollect` fusion.
- **CLI:** `job run` builds a `Job` and hands it to the executor.
- **Out of scope:** the on-disk `jobs.yml` migration to the phase shape is owned by
  `saved-job-migration` (ADR-0009); this change owns the in-memory model and executor.
- **Follow-up:** routing the remaining CLI and web surfaces through the executor, the
  stage-aligned module split, retiring `Collector` / `Processor` / `into_collect_exporter`,
  and child jobs as literal `model::Job`s are owned by **`executor-convergence`** (#368).
- **Depends on** `platform-application-split` (ADR-0001) for `Platform` propagation to
  child jobs.
