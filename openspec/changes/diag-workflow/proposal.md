## Why

The current web home page combines collection, processing, and delivery into a single "Process Diagnostics" panel, which makes it hard to support distinct workflows like collect-only, collect-and-send, or process-and-send. We need a clearer staged workflow now so users can choose how a diagnostic is sourced, whether it is processed, and where the resulting artifact is delivered without mode-specific behavior being hidden behind one form.

## What Changes

- Replace the single home page workflow panel with three explicit panels: `Collect`, `Process`, and `Send`.
- Give each stage two explicit operating modes:
  - `Collect`: `Collect` or `Upload`
  - `Process`: `Process` or `Forward`
  - `Send`: `Remote` or `Local`
- Make `Collect -> Collect` the remote intake path, supporting known-host selection in `user` mode, explicit URL/API-key entry, and Elastic Upload Service input, plus an optional `Save` toggle with a local directory target for persisting the collected archive before downstream steps.
- Reuse the existing `collect --save` behavior as the persistence mechanism that enables the collect-plus-save-plus-send workflow.
- Make `Collect -> Upload` the local intake path for drag-and-drop or file picker archive upload.
- Make `Process -> Process` expose diagnostic type selection plus advanced processor opt-in/out controls for only fully implemented processors.
- Keep required processing options locked on when advanced overrides are used so users cannot deselect dependencies or metadata/manifest-critical processors (for example Elasticsearch `version`, `cluster_settings_defaults`, or `node_settings` when `node_stats` is selected).
- Make `Process -> Forward` preserve the raw diagnostic archive unchanged from collection.
- Make `Send -> Remote` route processed diagnostics to a diagnostic cluster target or forwarded archives to a new Elastic Upload Service exporter path.
- Make `Send -> Local` route processed diagnostics to either a localhost diagnostic cluster target or a local directory, and disable local sending for forwarded archives while showing that the local bundle is saved in `Collect` and automatically enabling `Save` if needed.
- Treat `Remote` versus `Local` as a UI layer over the existing exporter choices and move the current footer `Output` target selection into the `Send` panel.
- Preserve current on-demand API retrieval for `collect + process + send` when `Save` is not enabled, but split the workflow into a collect job followed by a process-plus-send job when `Save` is enabled.
- Add a new `upload` CLI command for sending an unprocessed diagnostic bundle to Elastic Upload Service using `esdiag upload <file_name> <upload_id>`.

## Capabilities

### New Capabilities
- `diagnostic-workflow`: Define the three-panel home page workflow, staged state transitions, and collect/process/send execution rules for the web UI.
- `elastic-uploader`: Upload unprocessed diagnostic bundles to Elastic Upload Service from the CLI and workflow send stage.

### Modified Capabilities
- `api-selection`: Processing controls expose diagnostic product/type selection, only list fully implemented API options for advanced overrides, and preserve required processor dependencies.
- `collection-execution`: Workflow execution supports collect/upload intake, process/forward behavior, `--save`-backed archive persistence, single-job versus two-job orchestration, and local/remote delivery flows.
- `host-role-targeting`: Send target selection in the workflow is constrained to known hosts that are valid for the `send` phase, including localhost-only host targeting for local processed delivery.
- `web-runtime-modes`: Remote collection inputs and send target behavior remain mode-aware between `user` and `service` execution.

## Impact

- Web templates, Datastar signals, and server-side workflow handlers for the home page.
- Runtime workflow state that currently assumes a single form/tabbed intake surface instead of explicit two-option stages.
- Remote collection and processing orchestration, including optional `--save` archive persistence before processing or forwarding.
- Footer output-target behavior, which moves into the `Send` panel as a workflow choice instead of a global page control.
- Host filtering and send target resolution for Elasticsearch known hosts with the `send` role, plus localhost-only processed local delivery.
- New Elastic Upload Service exporter behavior and CLI upload entrypoint for unprocessed bundles.
- User/service mode UX and validation for remote collection credentials and local artifact usage.
