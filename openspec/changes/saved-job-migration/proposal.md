## Why

The universal `Job` model (ADR-0004) replaces the legacy `Job { collect, action }`
shape with a phase-structured `Job { input, save, process, send }`. Every existing
`jobs.yml` on disk still holds the old shape, and today's loader shape-sniffs an
untyped map. Without a plan, either old saved jobs break or the loader accumulates
ambiguous compatibility guesses forever. Rationale: **ADR-0009**.

## What Changes

- Add a `schema_version` field to `jobs.yml`. An **absent** version means **v1** —
  the legacy `Job { collect, action }` shape. This makes every future read
  deterministic: no shape-sniffing, and every migration gets the same versioned hook.
- On load, v1 entries are mapped to the phase-based `Job` via a **closed
  `From<LegacyJob>`** (ADR-0004). Every legacy saved job is collect-first, so
  `input` is always `Collect`; the legacy `action` maps as:
  - `Collect { output_dir }` → `save: Some(output_dir)`
  - `Upload { upload_id }` → `save: Some(dir)`, `send: Some(upload_id)`
  - `Process { output, selection }` → `save: save_dir?`, `process: Some { selection, export: output }`
- **Rewrite-on-first-read:** if any entry was legacy, the whole file is rewritten in
  the new shape using the existing atomic-write plumbing (`write_yaml_atomic` /
  `replace_file_atomic` / `secure_output_file`). Self-healing; no user action.
- **BREAKING (on-disk format, self-healing):** the persisted shape changes, but the
  first read migrates and rewrites transparently — users never re-create jobs.

## Capabilities

### New Capabilities

- _(none — this modifies the existing `saved-jobs` capability)_

### Modified Capabilities

- `saved-jobs`: version the persisted schema (`schema_version`, absent ⇒ v1); persist
  the phase-based `Job` (`input`/`save`/`process`/`send`) instead of `collect`+`action`;
  add a closed legacy→phase migration performed on load with rewrite-on-first-read.

## Impact

- **Persistence:** `src/data/saved_jobs.rs` — the `Job`/`JobAction` types, `SavedJobs`
  map, `load_saved_jobs`/`load_saved_jobs_async`, and the rewrite hook reusing the
  atomic-write helpers currently in `src/data/keystore.rs`.
- **On-disk `jobs.yml`:** gains `schema_version`; legacy entries rewritten in place on
  first read.
- **Migrated `Process` jobs without a `save_dir` become streaming** (no `Save`) rather
  than the legacy always-staged behavior — accepted deliberately (ADR-0009).
- **Out of scope:** `owner` (ADR-0008, execution-level, not authored into the saved
  definition) and the `Product` → `Platform`/`Application` split (ADR-0001) — only
  `JobProcessSelection.product` maps to `application`. Received artifacts
  (bundles/manifests) use read tolerance, not migration (ADR-0010).
