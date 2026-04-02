## Why

Users frequently configure multi-step diagnostic jobs in the `/jobs` web UI and need to re-run them without reconfiguring each time. Persisting named job configurations lets users build a library of reusable diagnostics and invoke them directly from the CLI.

## What Changes

- New `~/.esdiag/jobs.yml` file stores named job configurations (step sequences and their parameters).
- `/jobs` page gains a **Save** button that writes the current configuration to `jobs.yml` under a user-supplied name.
- `/jobs` page gains a narrow left-panel listing saved jobs; selecting one loads it into the steps configurator.
- New CLI subcommand `esdiag job run <jobname>` executes a saved job by name.
- Saved jobs require persisted known hosts from `hosts.yml` and remain a **User mode** feature only — unavailable in Service mode.

## Capabilities

### New Capabilities

- `saved-jobs`: Persist, list, and execute named diagnostic job configurations; includes CLI `job run`, `job list`, `job delete`, and `/jobs` page save/load UI.

### Modified Capabilities

- `web-runtime-modes`: Service mode omits the user-mode jobs workflow and saved-jobs UI.

## Impact

- **CLI**: New `job` subcommand with `run`, `list`, and `delete` under `esdiag`; parses `~/.esdiag/jobs.yml` at runtime.
- **Web UI**: `/jobs` page layout changes (left panel added); Save button wired to a new API endpoint.
- **Persistence**: New YAML file at `~/.esdiag/jobs.yml`; no database or server-side storage.
- **Runtime modes**: Service mode does not expose the user-mode jobs workflow or saved-jobs UI.
