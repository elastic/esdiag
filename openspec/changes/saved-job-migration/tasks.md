# Tasks

## 1. Versioned schema
- [ ] 1.1 Add a `schema_version` field to the persisted `jobs.yml` document (map wrapper), with the current schema version constant.
- [ ] 1.2 Serialize `schema_version` on every write in `save_saved_jobs` / the rewrite path.
- [ ] 1.3 Define the load-time dispatch: absent `schema_version` ⇒ v1 legacy; current version ⇒ direct deserialize. No shape-sniffing.

## 2. Legacy shape + closed mapping
- [ ] 2.1 Retain the legacy `Job { collect, action }` types as `LegacyJob` (deserialize-only) so v1 files parse.
- [ ] 2.2 Implement a closed, total `From<LegacyJob>` for the phase-based `Job` (ADR-0004): `input` always `Collect`.
- [ ] 2.3 Map `action: Collect { output_dir }` → `save: Some(output_dir)`.
- [ ] 2.4 Map `action: Upload { upload_id }` → `save: Some(dir)`, `send: Some(upload_id)`.
- [ ] 2.5 Map `action: Process { output, selection }` → `save: save_dir?`, `process: Some { selection, export: output }`; no `save_dir` ⇒ streaming (no `Save`).
- [ ] 2.6 Carry `JobProcessSelection` through unchanged; leave the `Product` → `Application` mapping to the platform/application split (out of scope here).

## 3. Rewrite-on-first-read
- [ ] 3.1 In `load_saved_jobs` (and `load_saved_jobs_async`), when the file is v1, migrate all entries and set the flag to rewrite.
- [ ] 3.2 If any entry was legacy, rewrite the whole file in the new shape reusing `write_yaml_atomic` / `replace_file_atomic` / `secure_output_file`.
- [ ] 3.3 Ensure the rewrite is idempotent: a second load of the rewritten file deserializes directly with no migration or rewrite.
- [ ] 3.4 Preserve the existing `saved_jobs_io_lock` guarantees so the read-then-rewrite is serialized.

## 4. Scope guards
- [ ] 4.1 Confirm the migration is invoked only for `jobs.yml`; do NOT apply it to bundles/manifests (received artifacts use read tolerance, ADR-0010).
- [ ] 4.2 Do NOT introduce `owner` into the saved schema (ADR-0008 — execution-level only).

## 5. Verification
- [ ] 5.1 Unit test: v1 file with each legacy `action` migrates to the expected phase-based `Job` (Collect / Upload / Process staged).
- [ ] 5.2 Unit test: legacy `Process` without `save_dir` migrates to a streaming job (no `Save`).
- [ ] 5.3 Unit test: loading a v1 file rewrites it once with `schema_version` set; a second load performs no rewrite (idempotent).
- [ ] 5.4 Unit test: a current-version file loads directly with no migration.
- [ ] 5.5 Test that a crash-safe atomic rewrite leaves the original intact on write failure (or assert reuse of the atomic helpers).
- [ ] 5.6 Confirm the delta spec scenarios in `specs/saved-jobs/spec.md` are covered.
