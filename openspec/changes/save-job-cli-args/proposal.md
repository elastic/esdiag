## Why

Operators need a direct CLI way to persist reusable saved jobs from commands they already run. Adding `--save-job <NAME>` to compatible collect/process invocations lets them save a job definition without using a separate UI or duplicate job-save grammar.

## What Changes

- Add `--save-job <NAME>` to `esdiag collect` and `esdiag process`.
- Persist a named `Job` before command execution by deriving an executable job from the effective invocation.
- Introduce a typed `Job`/`JobBuilder` domain model so saved jobs and one-shot executions can share valid execution shapes.
- Treat `SavedJobs` as YAML persistence for named `Job` values rather than a separate saved-job signal model.
- Reuse shared job validation and persistence rules so CLI and server paths enforce the same constraints.
- Reject `--save-job` on incompatible invocation shapes with clear non-zero failures.

## Capabilities

### Modified Capabilities

- `saved-jobs`: Add invocation-derived CLI job saving through `--save-job <NAME>` on `collect` and `process`, backed by the shared `Job` model.

## Impact

- Affected code: `src/main.rs`, `src/job.rs`, `src/data/saved_jobs.rs`, saved-job web adapters, and CLI docs.
- Affected user behavior: operators can create/update saved jobs inline while running collect/process commands.
- Not included: CLI bundle-retention behavior such as `--save-bundle`.
