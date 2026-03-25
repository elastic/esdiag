## Context

ESDiag's `/jobs` page lets users configure a multi-step diagnostic workflow (collect → process → send) using a rich Datastar-driven UI. Configurations are ephemeral today — every session starts from scratch. The `Workflow` struct captures the full three-stage state; `Identifiers` captures job metadata. Persistent storage already exists for `hosts.yml` and `settings.yml` under `~/.esdiag/`, both using Serde YAML. Runtime mode gating via `RuntimeModePolicy::allows_local_runtime_features()` already guards jobs/workflow pages from service mode.

## Goals / Non-Goals

**Goals:**
- Persist named job configurations to `~/.esdiag/jobs.yml` (YAML, Serde-compatible).
- Web UI: Save button + left-panel job list on `/jobs` page (User mode only).
- CLI: `esdiag job run <name>` executes a saved job using the existing CLI collect/process pipeline.
- CLI: `esdiag job list` lists saved job names.
- CLI: `esdiag job delete <name>` removes a saved job from `jobs.yml`.
- Selecting a saved job in the UI restores full workflow signal state.

**Non-Goals:**
- Job scheduling or cron-style execution.
- Shared/multi-user job storage.
- Service mode support.
- Editing a saved job in place (save overwrites by name).

## Decisions

### 1. Storage format: flat YAML map

`~/.esdiag/jobs.yml` is a map from job name → `SavedJob`. Consistent with `hosts.yml` and `settings.yml`; no new dependencies; human-editable.

```yaml
my-job:
  identifiers:
    account: "elastic"
    case_number: "12345"
  workflow:
    collect: { ... }
    process: { ... }
    send: { ... }
```

Alternative considered: SQLite for querying — rejected as over-engineered for what is a small named list.

### 2. SavedJob captures Workflow + Identifiers only

`Signals` contains transient state (auth tokens, loading flags, keystore state) that must not persist. Only `Workflow` and `Identifiers` are serialized. These two types will need `#[derive(Serialize, Deserialize)]`.

Alternative: serialize full `Signals` — rejected because it would leak session credentials and coupling web state to CLI execution.

### 3. New Axum routes for job persistence

Three new routes (all `allows_local_runtime_features()` gated):

| Method | Path | Action |
|--------|------|--------|
| `GET` | `/jobs/saved` | Render list fragment; return SSE `PatchElement` targeting `#saved-jobs-list` |
| `GET` | `/jobs/saved/:name` | Render full `/jobs` page with the named job's `Workflow` + `Identifiers` pre-populated in initial signals |
| `POST` | `/jobs/saved` | Save job by name; return SSE `PatchElement` refreshing `#saved-jobs-list` |
| `DELETE` | `/jobs/saved/:name` | Delete job by name; return SSE `PatchElement` refreshing `#saved-jobs-list` |

The list is always rendered server-side as an HTML fragment (Askama partial) and pushed to the DOM via `PatchElement`. No client-side signal holds the job list. The panel placeholder fires `GET /jobs/saved` on load via `data-on-load`; save and delete mutations each return a fresh `PatchElement` in-response.

### 4. Web UI: left panel via CSS layout, not a separate page

The `/jobs` template already uses a single-page Datastar component. The left panel is added as a sibling column in the existing layout using CSS (flexbox or grid), toggled visible only in User mode via a server-rendered conditional in the Askama template.

### 5. Only known-host collect sources are valid for saved jobs

Direct uploads (`LocalArchive`) and service link downloads (`FromServiceLink`) reference one-time paths/URIs. Saved jobs are intended to collect a _new_ diagnostic each time they run, so only `FromRemoteHost` (known host) is a valid collect source. The Save button SHALL be disabled when the workflow is configured for upload or service link collection.

### 6. Default job name derived from workflow state; user may override

The name field is pre-populated using the pattern `{host}-{action}-{destination}` computed from the current `Workflow` signals. The derivation is pure client-side (Datastar expression or small JS helper) — no round-trip needed since all inputs are already in signals. The user edits the field freely before saving. The server validates only that the submitted name is non-empty; overwrite is allowed (last-save wins).

| Workflow state | Default name |
|----------------|--------------|
| collect from `prod`, save locally | `prod-collect-save` |
| collect from `es_poc`, upload to service | `es_poc-collect-upload` |
| process, send to remote host `monitoring` | `prod-process-monitoring` |
| process, write to local directory | `prod-process-directory` |
| process, write to local file | `prod-process-file` |

### 7. `keystore` is a required feature for saved jobs

The shared runner needs to resolve secret-backed host credentials at execution time. The `keystore` feature MUST be a compile-time dependency for the saved jobs module. Users may be prompted to unlock the keystore when running `esdiag job run`.

### 8. Extract job state machine for CLI/web shared use

The collect → process → send execution flow currently lives inside `server/workflow.rs` and is tightly coupled to `ServerState` and SSE streaming. Rather than duplicating this logic for the CLI, the core job execution state machine SHALL be extracted into a standalone module (e.g., `job/runner.rs`) with no dependency on Axum or SSE. The web server and `esdiag job run` both call into this shared runner; the web layer wraps it with SSE progress events, the CLI layer writes progress to stdout.

This refactor is a prerequisite for `esdiag job run` and should be scoped as its own task group. The shared runner MUST compile and function correctly without the `server` feature flag — verified by building with `--no-default-features`.

Alternative considered: CLI calls into `workflow.rs` directly by constructing a minimal `ServerState` — rejected because it drags in web server dependencies and is not a clean abstraction boundary.

## Risks / Trade-offs

- **Serde compatibility drift** → Adding fields to `Workflow` or `Identifiers` may break existing `jobs.yml` files. Mitigation: use `#[serde(default)]` on all new fields.
- **Left panel width** → Narrow panel may not fit long job names on small screens. Trade-off accepted; truncation with tooltip is sufficient for v1.
- **Stale host references on load** → A saved job may reference a host that has since been removed from `hosts.yml`. On web load (`GET /jobs/saved/:name`) the page renders normally but the stale host input is highlighted as invalid (same invalid-field styling used elsewhere). On CLI (`esdiag job run`, `esdiag job list`) stale references are highlighted in red in terminal output; `job run` returns a clear error.
- **UI changes land in `components/jobs.html`** → The `jobs.html` page template is a thin shell (`{% include "components/jobs.html" %}`); all layout and signal changes go in the component file.
- **Concurrent file access** → The web server and CLI both read/write `jobs.yml`. Simultaneous writes (e.g., web save + CLI delete) could clobber each other. Accepted as exceptionally rare in practice; same limitation as `hosts.yml`.
