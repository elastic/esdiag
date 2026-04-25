## Context

Saved jobs are already persisted in `~/.esdiag/jobs.yml` and can be listed/run/deleted. The missing piece is direct CLI capture of collect/process invocations so users do not need a separate web flow just to save reusable jobs.

The execution shape should not be specific to saved jobs. "Saved" only means a job was serialized to YAML; the same `Job` model should also be usable for one-shot CLI or UI execution.

## Decisions

### 1. Keep save-job invocation-derived

`--save-job <NAME>` attaches to existing collect/process commands instead of introducing a parallel `job save` grammar.

Why:
- avoids duplicate argument models for collect/process/send
- ensures saved definitions match the command users actually run
- keeps CLI docs and shell previews straightforward

### 2. Save first, execute second

When `--save-job` is present, the command derives and persists the saved job before continuing with normal command execution.

Why:
- guarantees a successful run reflects a persisted reusable definition
- surfaces compatibility errors early and clearly

### 3. Model executable jobs with typed actions

Persisted jobs should store a strict executable shape instead of the broad UI signal state. `Job` contains collection input plus a `JobAction` enum such as collect-only, collect-and-upload, or collect-process-send.

Why:
- removes invalid combinations like disabled processing plus unrelated send targets
- avoids string sentinels such as `local_target == "directory"`
- makes saved and one-shot execution use the same domain model

### 4. Use JobSignals as the UI boundary

`JobSignals` represents the Datastar signal payload for the web UI. `Job` conversion uses `JobBuilder` so UI signal payloads and CLI arguments share validation before producing an executable `Job`.

Why:
- keeps CLI/UI parsing flexible while storage and execution stay strict
- makes missing required fields fail before persistence
- follows the repository preference for typestate-style builders

### 5. Keep bundle retention separate from final output

`save_dir` is optional retention for an intermediate diagnostic bundle, such as a future `--save-bundle <PATH>` flow. `output_dir` is an action output: it is required for collect actions and appears on process output only when the process target is a directory. The web UI uses a `download_dir` signal for the user's browser/download choice; saving that state maps it to `save_dir` only when the bundle is retained before a process or upload action, and maps it to collect `output_dir` when the download is the final collect action.

Why:
- avoids persisting the same collect path as both retention and final output
- lets process/upload jobs use temporary bundle storage unless retention is explicitly requested
- keeps directory output semantics visible in the job YAML

### 6. Centralize save validation in job module

CLI save path uses `esdiag::job::save_job`, which validates job names before writing `jobs.yml`.

Why:
- keeps name validation shared and consistent
- makes persistence independent from how a `Job` was built

## Risks / Trade-offs

- Job building intentionally supports only invocations that map to known-host collection jobs.
- Process jobs require explicit output targets so execution has deterministic send behavior.
- Users compose save behavior directly through CLI flags; this change does not add another job authoring surface.
