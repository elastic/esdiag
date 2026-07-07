# Design

Full rationale, rejected alternatives, and consequences live in
**`docs/adr/0002-unify-operations-into-one-six-stage-pipeline.md`**,
**`docs/adr/0003-name-the-universal-model-job.md`**, and
**`docs/adr/0004-universal-job-model-and-stage-aligned-modules.md`**. This design covers
only the implementation approach.

## Context

`handle_job_run` requires a collect host (no `Load` input); every `JobAction` variant
collects to a bundle first (always staged, never streaming); and `JobAction` fuses the
phases into three mutually-exclusive variants that cannot express Save-and-Process
together. `Collector` and `Processor` are separate runtime types with duplicated
collect/stream logic, and the CLI-streaming path and the job-staged process path are two
code paths for what is one operation.

## Model

```
Job {
  identifiers,
  input:   Collect | Load,        // Phase 1, required
  save:    Option<SaveTarget>,    // Phase 2a: raw bundle
  process: Option<Process>,       // Phase 2b: transform + its Export sink
  send:    Option<SendTarget>,    // Phase 3: bundle -> Elastic Uploader
}
Process { selection, export: ExportTarget }   // Export lives inside Process (Model β)
```

## Approach

- A new `job/` module owns the `Job` type, the phase enums, a **validated constructor**
  (invariants below), and an `executor` that derives staged vs streaming and drives the
  stages. Both the CLI and the web build a `Job` and hand it to this one executor.
- `receiver/` resolves Phase-1 input uniformly for both `Collect` (remote, uses a client)
  and `Load` (local/download, no client).
- `processor/` is transform-only (per-API processors); it no longer owns collection or
  orchestration of sinks.
- `exporter/` is typed by role: `BundleExporter` (`Save`, raw) and `DocumentExporter`
  (`Export`, processed). Role-typing makes the invalid pairings — processed-docs-to-bundle
  and raw-to-cluster — unrepresentable.
- `uploader.rs` is `Send`.
- Retire `into_collect_exporter` and the `JobAction`/`JobCollect` fusion; the executor
  selects its strategy from the derived mode rather than from a fused action variant.

## Invariants (enforced at construction)

- `save` ⟹ `input` is `Collect` — you save only what you newly collected.
- `send` ⟹ a bundle exists (`Load` input, or `save` set).
- at least one of `save` / `process` / `send` is set — a job must do something.
- `Export` ⟺ `Process` — structural, because `Export` lives inside `Process`.

Everything the invariants exclude is unrepresentable in the type, not runtime-checked.

## Execution mode (derived, not stored)

- `save` + `process` ⇒ **staged**: collection completes and the bundle materialises (the
  serialization barrier) before processing reads it.
- `Collect` + `process` without `save` ⇒ **streaming**: receive, transform, and export
  overlap concurrently, reusing the existing `get_stream` / `StreamingDataSource` /
  `document_channel` machinery.
- `send` composes with either: a staged or `Load`-input job transmits the materialised or
  loaded bundle; a streaming job never has a bundle to send.

## Child jobs

Included diagnostics spawn as child `Job`s — a `Load` input over the nested diagnostic
plus a `Process` stage — each minting a child `JobID` and driven by the same executor. The
parent sets each child's `Platform` as it spawns it (per `platform-application-split`).
Inclusion stays one level deep (unchanged).

## Risks

- **Wide blast radius.** Retiring `Collector`/`Processor` touches collect, process, the CLI,
  and the web surfaces. Mitigate by landing the `job/` executor behind the existing surfaces
  first, then removing the old paths once both drive the executor.
- **Streaming/staged convergence.** The one executor must preserve current streaming
  concurrency and backpressure (`document_channel`); a regression would surface as memory or
  throughput change. Cover with the existing streaming tests before retiring the old path.
- **Persistence coordination.** The in-memory phase shape must match the on-disk shape that
  `saved-job-migration` (ADR-0009) migrates to; land the model definition here and the
  migration there against the same `Job`.
- **UI projection.** Collapsing `JobSignals` risks web regressions; keep the UI verbs stable
  as a presentation projection over the phases rather than a parallel model.
