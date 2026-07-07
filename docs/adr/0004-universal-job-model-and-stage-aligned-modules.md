---
type: Reference
title: "Universal `Job` model and stage-aligned module layout"
status: accepted
tags: [repository, adr]
---

# Universal `Job` model and stage-aligned module layout

`Job` becomes the single execution model shared by the CLI, the web server, and the
executor (ADR-0002 defines the six stages; ADR-0003 the name). Today's `Job` is a
strict subset of that space — `handle_job_run` requires a collect host (no `Load`
input), every action collects to a bundle first (always staged, never streaming),
and `JobAction` fuses phases into three mutually-exclusive variants that cannot
express Save-and-Process together. We replace it with a phase-structured `Job` and
a single executor, and realign modules to the stages.

## Model

```
Job {
  identifiers,
  input:   Input,               // Phase 1, required: Collect | Load
  save:    Option<SaveTarget>,   // Phase 2a: raw bundle
  process: Option<Process>,      // Phase 2b: transform + its Export sink
  send:    Option<SendTarget>,   // Phase 3: bundle -> Elastic Uploader
}
Process { selection, export: ExportTarget }   // Export lives inside Process
```

`Export` is fused into `Process` (**Model β**), so "Export ⟺ Process" is a
type-level guarantee — "process to nowhere" and "export nothing" are
unrepresentable. `Export` and `Send` are independent and may both run in one job
(Phase 3 is *and/or*, not xor).

Invariants validated at construction (the rest are unrepresentable):

- `save` ⟹ `input` is `Collect`
- `send` ⟹ a bundle exists (`Load` input, or `save` set)
- at least one of `save` / `process` / `send` is set

Execution mode is *derived*, not stored: `save` + `process` → staged (process the
materialised bundle); `Collect` + `process` without `save` → streaming.

## Considered options

- **Enumerate valid job shapes as one big enum** (one variant per known workflow).
  Rejected: rigid, duplicative, and re-introduces named operation types that
  ADR-0002 dissolves.
- **`Export` as a separate Phase-3 peer** (`Output { Export | Send }`, Model α).
  Rejected: matches the earlier "Phase 3" phrasing but leaves "process with no
  output" as a runtime-checked invalid state instead of an impossible one.
- **Phase-structured `Job` with `Export` inside `Process` (chosen, Model β).**

## Consequences

- **Modules align to stages, with one executor:**
  - `job/` (new) — the `Job` model, phase types, validated construction, and
    `executor` (derives staged vs streaming; drives the stages)
  - `receiver/` — extract: `Collect` sources (live) + `Load` sources (bundle)
  - `processor/` — transform: per-product diagnostic `Process` only
  - `exporter/` — load sinks, split by role: `BundleExporter` (`Save`) +
    `DocumentExporter` (`Export`)
  - `uploader.rs` — `Send`
  - `data/saved_jobs.rs` — persistence of *named* `Job`s only
- **Retired:** `Collector`, `Processor` (as distinct operation types), `JobAction`,
  `JobCollect`, `into_collect_exporter`, and the duplicate CLI-streaming /
  job-staged process paths (they converge on the one executor).
- **New capabilities fall out for free:** `Load`-input jobs, streaming jobs, and
  Save + Process + Export + Send in a single run — none expressible today.
- **`JobSignals` collapses** to a thin UI projection of `Job`, or is removed; the
  web form binds the `Job` phases directly.
