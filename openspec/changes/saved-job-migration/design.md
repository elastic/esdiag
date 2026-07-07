# Design

Full rationale, rejected alternatives, and consequences live in
**`docs/adr/0009-migrate-saved-jobs-by-rewrite-on-first-read.md`** (the phase-based
target `Job` is defined by **ADR-0004**); this design covers only the implementation
approach.

## Context

`jobs.yml` is a YAML map from name to the legacy `Job { identifiers, collect: JobCollect,
action: JobAction }` (`src/data/saved_jobs.rs`), where `JobAction` fuses phases into three
mutually-exclusive variants (`Collect`/`Upload`/`Process`). ADR-0004 replaces this with a
phase-structured `Job { input, save, process, send }`. Every persisted file is legacy and
carries no version marker; the loader (`load_saved_jobs`) currently deserializes the map
directly with no versioning hook.

## Approach

- **Version the file.** Add `schema_version` to the persisted payload. An **absent**
  value means **v1** (legacy). This is the deterministic dispatch point: v1 → migrate;
  current version → deserialize directly. No shape-sniffing, and every future schema bump
  reuses the same hook.
- **Closed mapping.** A `From<LegacyJob>` maps each v1 entry to the phase-based `Job`.
  Every legacy saved job is collect-first (the old `handle_job_run` required a host), so
  `input` is always `Collect`. The `action` maps as:

  | Legacy `action` | New phases |
  |---|---|
  | `Collect { output_dir }` | `save: Some(output_dir)` |
  | `Upload { upload_id }` | `save: Some(dir)`, `send: Some(upload_id)` |
  | `Process { output, selection }` | `save: save_dir?`, `process: Some { selection, export: output }` |

  `JobCollect.save_dir` (when present) supplies the `Save` target for the `Process` case.
- **Rewrite-on-first-read.** If any entry was legacy, the loader migrates the whole map
  and rewrites `jobs.yml` in the new shape (with `schema_version` set) using the existing
  atomic-write plumbing — `write_yaml_atomic` → `secure_output_file` → `replace_file_atomic`
  (`src/data/keystore.rs`). One rewrite converges the file; subsequent reads are direct.

## Invariants

- `schema_version` absent ⟺ v1 legacy shape; present ⟺ current phase-based shape.
- The migration is **total** over v1 inputs: every legacy `action` has exactly one target
  mapping (closed `From`, no fallthrough).
- Every migrated job satisfies ADR-0004 construction invariants: `save ⟹ input=Collect`
  (holds — input is always `Collect`); `send ⟹ a bundle exists` (holds — `Upload` sets
  `save`); at least one of `save`/`process`/`send` is set.
- Rewrite is all-or-nothing per file (atomic replace); a crash mid-write leaves the
  original intact.

## Risks / trade-offs

- **Migrated `Process` jobs without a `save_dir` become streaming** (no `Save`) rather
  than the legacy always-staged behavior. Accepted deliberately (ADR-0009): same result,
  strictly better; the execution-mode change is fine.
- **Owned-file strategy only.** This rewrite applies to files ESDiag owns and writes.
  Received artifacts (bundles/manifests) are read-only and use additive read tolerance
  instead (ADR-0010) — do not apply migration to them.
- **Out of scope for the saved schema:** `owner` (ADR-0008) is execution-level and job
  authoring is `User`-mode-only, so it never enters the definition; the `Product` →
  `Platform`/`Application` split (ADR-0001) touches saved jobs only via
  `JobProcessSelection.product` → `application`, handled with the broader split.
