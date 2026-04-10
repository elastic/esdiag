## Why

Agent skills are becoming a first-class way to drive `esdiag`, but the current CLI still optimizes for human-readable logs rather than low-noise agent execution. It also builds the final Kibana link from `ESDIAG_KIBANA_URL` alone, which ignores the saved host role metadata we now have and prevents processed send runs from using an explicitly related viewer host when one is configured.

## What Changes

- Add a parent-level `--agent` / `-a` CLI mode that reduces log noise for agentic runs by forcing warn-level logging unless debug mode is explicitly requested.
- Make successful CLI commands emit their final human-readable completion summary through an explicit `STDERR` print path instead of only surfacing it through an info-level log message.
- Auto-enable agent mode when the `CLAUDECODE` environment variable is present so agent-driven invocations get the low-noise behavior without extra flags.
- Update processed diagnostic completion reporting for `send` hosts to resolve the returned Kibana link from the host's saved `viewer` reference before falling back to `ESDIAG_KIBANA_URL`.
- Preserve existing non-agent CLI behavior and the environment-variable Kibana URL fallback when no saved viewer host is configured.

## Capabilities

### New Capabilities
- `cli-agent-mode`: Define parent CLI agent mode, low-noise logging defaults, tracing-independent final completion output on `STDERR`, and `CLAUDECODE` auto-enable behavior.

### Modified Capabilities
- `diagnostic-reporting`: Final processing/report output includes viewer-aware Kibana link resolution for processed send runs and preserves explicit fallback behavior.
- `host-role-targeting`: Saved host relationships support resolving a send host's `viewer` reference to a valid Kibana view target for post-processing link generation.

## Impact

- CLI argument parsing and startup behavior in `src/main.rs`, including parent command flags and logging initialization.
- Final result rendering for CLI processing flows so human-readable summaries are available regardless of log level without contaminating streamed `STDOUT` document output.
- Saved host resolution and role-aware viewer lookup used when building Kibana links for processed diagnostic output.
- Regression coverage for agent mode activation, `CLAUDECODE` auto-enable, `STDERR` summary behavior, and viewer-based Kibana link selection with environment fallback.
