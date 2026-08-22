---
type: Reference
title: "Migrate `saved_jobs.yml` by rewrite-on-first-read, versioned"
status: accepted
tags: [repository, adr]
---

# Migrate `saved_jobs.yml` by rewrite-on-first-read, versioned

`saved_jobs.yml` gains a `schema_version` field; an **absent** version means v1 (the
legacy `Job { collect, action }` shape). On load, v1 entries are mapped to the
phase-based `Job` of ADR-0004 via a closed `From<LegacyJob>`, and if any entry was
legacy the whole file is **rewritten in the new shape on first read**. This is the
right call despite a small user base because the mapping is cheap and a clean,
self-healing UX beats making users re-create jobs.

## Mapping

Every legacy saved job is collect-first (old `handle_job_run` required a host), so
`input` is always `Collect`:

| Legacy `action` | New phases |
|---|---|
| `Collect { output_dir }` | `save: Some(output_dir)` |
| `Upload { upload_id }` | `save: Some(dir)`, `send: Some(upload_id)` |
| `Process { output, selection }` | `save: save_dir?`, `process: Some { selection, export: output }` |

## Consequences

- **`schema_version` makes all future reads deterministic** — no shape-sniffing;
  every migration gets the same versioned hook.
- **Totality is what makes the migration safe to run unattended.** The whole file is
  loaded or none of it is, so a single entry the mapping cannot handle would cost the
  user every saved job at once — and only on the upgrade that migrates. Nothing about
  the *current* state of the codebase may therefore reject a v1 entry: a legacy
  `selection` naming a product or source the source registry no longer knows is
  carried through as authored, and `job run` rejects that one job with an actionable
  error, rather than the loader rejecting the file.
- **Migrated `Process` jobs without a `save_dir` become streaming** (no `Save`)
  rather than the legacy always-staged behavior. Accepted deliberately — same
  result, strictly better; execution-mode change is fine.
- **`owner` (ADR-0008) is out of scope** — it is an execution-level property, and
  job authoring is `User`-mode-only, so the saved schema is unaffected.
- **`Product` → `Platform`/`Application` (ADR-0001) barely touches saved jobs** —
  only `JobProcessSelection.product` maps to `application`.
- **This strategy applies only to files ESDiag owns and writes.** Bundles/manifests
  are received read-only and use read tolerance instead (ADR-0010).
- The existing atomic-write plumbing (`write_yaml_atomic` / `replace_file_atomic` /
  `secure_output_file`) is reused for the rewrite-back.
