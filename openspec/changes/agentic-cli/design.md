## Context

`esdiag` currently exposes only one parent CLI mode switch, `--debug`, and otherwise initializes tracing from `LOG_LEVEL` or the default `info` level. That works for human-driven terminal use, but it is noisy for agentic execution because the final command result is mostly surfaced through info-level tracing output. At the same time, the processing pipeline builds the optional final Kibana link inside `Processor::process()` by reading `ESDIAG_KIBANA_URL` directly, even though saved hosts already support validated `viewer` references from `send` hosts to `view` hosts.

This change is cross-cutting but intentionally small in scope. It affects CLI startup in `src/main.rs`, processing completion reporting in `src/processor/mod.rs`, and saved-host lookup behavior in `src/data/known_host.rs`. The design must preserve current interactive behavior, avoid schema changes to `hosts.yml`, and keep the fallback environment-based Kibana URL path for installations that do not use saved viewer hosts.

## Goals / Non-Goals

**Goals:**
- Add a global `--agent` / `-a` switch that activates a low-noise CLI mode.
- Auto-enable the same mode when `CLAUDECODE` is present.
- Make agent mode force warn-level logging unless `--debug` is explicitly set.
- Make the CLI emit a final human-readable completion summary to `STDERR` through an explicit print path instead of relying on info-level tracing output.
- Resolve the final processed-diagnostic Kibana link from a `send` host's saved `viewer` reference before falling back to `ESDIAG_KIBANA_URL`.
- Preserve the existing `ESDIAG_KIBANA_SPACE` suffix behavior when a Kibana base URL is available from either source.

**Non-Goals:**
- Introduce a new machine-readable output format or JSON-only CLI mode.
- Change web UI runtime behavior or server-side settings persistence.
- Alter the saved host schema or validation rules beyond consuming the existing `viewer` relationship at runtime.
- Remove the environment-variable Kibana URL fallback for users who do not use saved hosts.

## Decisions

### 1. Derive an explicit CLI execution mode before tracing initialization

The CLI will add a global `agent: bool` flag and compute an execution-mode decision immediately after parsing:
- `--debug` remains the highest-precedence explicit logging control.
- Otherwise, agent mode is active when `--agent` is passed or `CLAUDECODE` exists in the environment.
- When agent mode is active, the tracing subscriber uses `warn` as the effective filter instead of reading the usual default/info path.

Rationale:
- The user asked for `--agent` to be a shortcut for `LOG_LEVEL="warn"`, so the behavior should be decided at the same point where logging is configured.
- The `CLAUDECODE` auto-enable path should behave exactly like an explicit flag rather than as a later conditional inside command execution.
- Keeping this logic at startup avoids having individual subcommands reinterpret logging behavior independently.

Alternatives considered:
- Mutate `LOG_LEVEL` in-process and continue reading it back through the existing code path. Rejected because it hides the effective precedence rules and couples runtime behavior to environment mutation.
- Treat agent mode as only an output-mode toggle and keep info logging enabled. Rejected because it would not reduce token usage meaningfully.

### 2. Resolve the processing Kibana base URL before entering the processor

The CLI process command will resolve an optional Kibana base URL from command context before creating the processor:
- If the explicit output target resolves to a saved `send` host and that host has a `viewer` reference, load the referenced saved host and use its URL as the Kibana base.
- If no viewer-backed Kibana base can be resolved, fall back to `ESDIAG_KIBANA_URL`.
- If `ESDIAG_KIBANA_SPACE` is present, append `/s/<space>` to whichever base URL is selected.
- Pass the resolved optional Kibana base into the processing path instead of having `Processor::process()` read the environment directly.

Rationale:
- The viewer relationship is already validated in saved-host storage, so runtime resolution can rely on an existing contract instead of inventing new metadata.
- Resolving the URL before processor execution keeps the processor deterministic and avoids hardcoding CLI-specific host lookup inside deeper processing code.
- Preserving the env fallback ensures existing automation keeps working when no saved viewer host is configured.

Alternatives considered:
- Keep environment lookup inside `Processor::process()` and add a second lookup there for saved hosts. Rejected because it mixes CLI host resolution with shared processing logic and hides the source of the chosen URL.
- Resolve the Kibana URL from the exporter after conversion to `Exporter`. Rejected because the exporter intentionally abstracts transport details and no longer retains the saved-host relationship cleanly enough for viewer lookup.

### 3. Reuse the completed diagnostic report as the final CLI summary source

The CLI will emit a single final human-readable summary to `STDERR` after successful command completion, regardless of whether agent mode is active:
- For `process`, the summary will be built from the completed `DiagnosticReport`, including the document count, diagnostic id, runtime, and Kibana link when present.
- For commands that already have a natural terminal success message (`host`, `keystore`, `upload`, etc.), the top-level command handler will print one concise completion line to `STDERR` after the action succeeds.
- This summary path must bypass tracing/log filtering entirely so it still appears when `LOG_LEVEL=warn` or when `STDOUT` is reserved for streamed `.ndjson` documents.

Rationale:
- The report already contains the canonical post-processing outcome, including the final Kibana link, so it is the right source of truth for the process summary.
- A single explicit `STDERR` completion message gives agents and human operators a stable terminal result without requiring broad changes to all existing progress logging.
- Routing the summary to `STDERR` preserves the existing `STDOUT` contract for streamed document output and shell pipelines.

Alternatives considered:
- Print every important info-level message to `STDOUT` in agent mode. Rejected because it recreates the noise problem the flag is meant to solve and breaks `STDOUT` piping for streamed documents.
- Add a new structured JSON emitter. Rejected because the request only calls for an explicit unfiltered terminal summary, not a new output contract.

## Risks / Trade-offs

- **[Risk] Agent mode could unexpectedly suppress useful logs during troubleshooting** -> **Mitigation:** Keep `--debug` as an explicit override that restores verbose tracing even when `--agent` or `CLAUDECODE` is active.**
- **[Risk] Viewer resolution may fail if the saved host inventory changes between CLI parse and process execution** -> **Mitigation:** Resolve the viewer host immediately before processor creation and fall back to the environment-derived Kibana URL when no valid runtime match is available.**
- **[Risk] Multiple command handlers may drift in how they phrase final `STDERR` summaries** -> **Mitigation:** Centralize the final `STDERR` emission behind small helper functions in `main.rs` rather than hand-formatting in each branch.**
- **[Risk] Moving Kibana URL selection out of `Processor::process()` could affect non-CLI callers** -> **Mitigation:** Keep the processor-side interface optional and default-safe so existing callers can continue omitting a Kibana base when they do not need link generation.**

## Migration Plan

1. Add the global CLI agent flag plus startup execution-mode resolution and tracing filter selection.
2. Introduce helper functions for resolving a process-command Kibana base URL from output host context plus environment fallback.
3. Thread the optional resolved Kibana base through processor execution and keep the existing report link-building logic, but source it from passed context instead of direct environment reads.
4. Add tracing-independent `STDERR` summary helpers and call them from successful command paths.
5. Add regression coverage for `--agent`, `CLAUDECODE`, viewer-based Kibana link selection, environment fallback, and `STDERR` final output behavior without `STDOUT` contamination.

Rollback strategy:
- Remove the global agent-mode branch and return to the current tracing-only completion behavior.
- Revert the optional processor Kibana-base parameter and resume environment-only link generation.
- No data migration is required because saved host files and reports remain backward-compatible.

## Open Questions

None at proposal time.
