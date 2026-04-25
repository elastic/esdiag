## 1. CLI Save-Job Arguments

- [x] 1.1 Add `--save-job <NAME>` to `collect` and `process` command arguments.
- [x] 1.2 Derive and persist saved jobs from compatible collect invocations.
- [x] 1.3 Derive and persist saved jobs from compatible process invocations with explicit outputs.

## 2. Shared Saved-Job Persistence

- [x] 2.1 Add shared `save_job` helper in `src/job.rs` for name/job validation and persistence.
- [x] 2.2 Reuse existing job compatibility checks and add explicit name validation.
- [x] 2.3 Replace the flattened saved-job signal payload with a shared typed `Job` domain model.
- [x] 2.4 Add a typestate-style `JobBuilder` that converts CLI/UI signal inputs into executable jobs.
- [x] 2.5 Update saved-job persistence to store named `Job` values in `jobs.yml`.
- [x] 2.6 Update saved-job execution/listing and web adapters to consume `Job`, using `JobSignals` as UI form state.

## 3. Documentation and Verification

- [x] 3.1 Update CLI reference docs for `collect --save-job` and `process --save-job`.
- [x] 3.2 Add/adjust tests for CLI parsing and derivation guardrails.
- [x] 3.3 Run `cargo test` for affected paths.
- [x] 3.4 Add/adjust tests for `Job` serialization, builder validation, and job host reference tracking.
- [x] 3.5 Re-run affected tests after the `Job`/`JobBuilder` refactor.
