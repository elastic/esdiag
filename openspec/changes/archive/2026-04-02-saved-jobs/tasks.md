## 1. Data Model

- [x] 1.1 Add `#[derive(Serialize, Deserialize)]` to `Workflow`, `CollectStage`, `ProcessStage`, `SendStage`, and `Identifiers` types
- [x] 1.2 Define `SavedJob` struct containing `identifiers: Identifiers` and `workflow: Workflow`; add `#[serde(default)]` on all fields
- [x] 1.3 Define `SavedJobs` type alias as `IndexMap<String, SavedJob>` (preserves insertion order)
- [x] 1.4 Implement `SavedJobs::load()` and `SavedJobs::save()` helpers that read/write `~/.esdiag/jobs.yml` (following pattern from `KnownHost::load` / `Settings`)

## 2. Web API Routes

- [x] 2.1 Add Askama partial `saved_jobs_list.html` that renders the jobs list HTML fragment (iterates `SavedJobs`, handles empty state inline)
- [x] 2.2 Add `GET /jobs/saved` handler that reads `jobs.yml` and returns an SSE `PatchElement` with the rendered list fragment targeting `#saved-jobs-list`
- [x] 2.3 Add `POST /jobs/saved` handler accepting `{ name: String, job: SavedJob }`, validates non-empty name plus known-host eligibility, writes to `jobs.yml`, returns SSE `PatchElement` refreshing `#saved-jobs-list`
- [x] 2.4 Add `DELETE /jobs/saved/:name` handler removing the named entry from `jobs.yml`, returns SSE `PatchElement` refreshing `#saved-jobs-list`
- [x] 2.5 Register all routes inside the `allows_local_runtime_features()` guard block in `server/mod.rs`

## 3. Web UI — Left Panel

- [x] 3.1 Add a left-panel column to `templates/components/jobs.html` using a flex/grid layout; gate it with an Askama `{% if runtime_mode.allows_local_runtime_features() %}` block
- [x] 3.2 Add `<div id="saved-jobs-list" data-on-load="@get('/jobs/saved')"></div>` placeholder in the panel; the server renders and patches the list HTML on load and after each mutation
- [x] 3.3 In `saved_jobs_list.html`: each item has a **Load** link (`href="/jobs/saved/:name"` navigates to the full page pre-rendered with that job) and a **Delete** button (`data-on-click` → `DELETE /jobs/saved/:name` returns `PatchElement` refreshing `#saved-jobs-list`)
- [x] 3.4 Add a **Job name** text input (signal `job_name: String`) and a **Save** button; Save action posts to `POST /jobs/saved` which returns `PatchElement` refreshing `#saved-jobs-list`
- [x] 3.5 Derive the default `job_name` signal value from workflow signals using the `{host}-{action}-{destination}` pattern; recompute reactively as the workflow configuration changes (collect/process mode, source host, send target)
- [x] 3.6 Disable the Save button when the collect source is anything other than a selected known host

## 4. Load-Job Route

- [x] 4.1 Add `GET /jobs/saved/:name` handler that loads the named job from `jobs.yml`, merges its `Workflow` and `Identifiers` into the initial `Signals`, and renders the full `/jobs` page with that state pre-populated
- [x] 4.2 When the named job is not found, render the `/jobs` page with a "Job <name> not found" message
- [x] 4.3 When the loaded job references a host not present in `hosts.yml`, render the page normally but mark the stale host signal so the host input field renders with invalid-field styling (same CSS class used for other validation errors)

## 5. Job Runner Extraction (prerequisite for CLI run)

- [x] 5.1 Identify the core execution path in `server/workflow.rs` — the collect → process → send state transitions — and define its inputs and outputs independent of `ServerState` and SSE
- [x] 5.2 Extract the state machine into `job/runner.rs` (or equivalent) with a public API that accepts a `SavedJob` (or equivalent config) and a progress callback/channel; no Axum or SSE dependencies
- [x] 5.3 ~~Refactor `server/workflow.rs` to delegate to the shared runner~~ — kept `workflow.rs` unchanged; standalone `job.rs` runner uses same library primitives
- [x] 5.4 Verify web job execution still works after refactor

## 6. CLI — `job` Subcommand

- [x] 6.1 Add `Job { #[command(subcommand)] command: JobCommands }` variant to the `Commands` enum in `main.rs`
- [x] 6.2 Define `JobCommands` enum with `Run { name: String }`, `List`, and `Delete { name: String }` variants
- [x] 6.3 Implement `handle_job_list()`: loads `jobs.yml`, renders a text table with columns: **Name**, **Collection target**, **Processing level** (standard / support / skipped / etc.), **Send target**; host references not found in `hosts.yml` are highlighted in red (ANSI); empty file prints nothing
- [x] 6.4 Implement `handle_job_run(name)`: loads `jobs.yml`, looks up the named job, resolves host credentials via keystore, invokes the shared runner from §5 with stdout progress; return clear eyre errors for missing file, unknown name, stale host reference, and keystore unlock failure
- [x] 6.5 Implement `handle_job_delete(name)`: loads `jobs.yml`, removes the named entry (error if not found), writes back
- [x] 6.6 Wire `Commands::Job` → `handle_job_list` / `handle_job_run` / `handle_job_delete` in `main.rs`
- [x] 6.7 Block keystore secret deletion when the secret is still referenced by any saved host or saved job, and surface those references in the error message

## 7. Verification

- [x] 7.1 Run `cargo clippy` and fix all warnings
- [x] 7.2 Run `cargo test` and fix any failures
- [x] 7.2a Add regression tests covering protected secret deletion when hosts or saved jobs still reference the secret
- [x] 7.3 Manually verify: save a job in the UI, reload the page, load the job; confirm `jobs.yml` is written correctly
- [x] 7.4 Manually verify: `esdiag job run <name>` executes the saved job end-to-end
- [x] 7.5 Build without `server` feature (`cargo build --no-default-features`) and verify `esdiag job run <name>` still compiles and executes correctly; confirms the shared runner has no server-feature dependency
- [x] 7.6 Manually verify: `esdiag job delete <name>` removes the entry from `jobs.yml`
- [x] 7.7 Manually verify: service mode does not expose the user-mode `/jobs` or `/workflow` pages
