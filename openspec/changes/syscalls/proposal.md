## Why

`collect` currently relies on API-based diagnostics and misses host-level context that helps explain local runtime issues. Aligning syscall collection with the existing Java implementation (derive runtime variables from `_nodes` and local host matching) improves diagnostic completeness without changing user workflow.

## What Changes

- Use `_nodes` API output plus local host/network identity matching to determine which cluster node corresponds to the machine running `collect`.
- Use deterministic first-match order when multiple `_nodes` entries satisfy local host/network matching.
- Resolve `{{LOGPATH}}` from `nodes.<id>.settings.path.logs` and `{{PID}}` from `nodes.<id>.process.id` for the matched node.
- Resolve `{{JAVA_HOME}}` from `ps -ef` (or OS equivalent) using the resolved PID, taking the first executable path in the process command line.
- Execute OS-specific syscall commands from `assets/elasticsearch/syscalls.yml` after variable resolution based on matched node/process context.
- Add receiver-driven raw syscall collection flow (pure collection text, no parsing) using existing raw save/retrieval patterns.
- Introduce a dedicated syscall data-source path type (for example `PathType::SystemCall`) to distinguish system command execution from file and URL sources.
- Include syscall command outputs in the same diagnostic bundle produced for API collection (no separate bundle and no new CLI flag).
- Treat syscall command failures/timeouts as non-fatal and emit warnings while continuing collection.
- Keep behavior cross-platform (Linux, macOS, Windows) by selecting commands from `assets/elasticsearch/syscalls.yml` for the current OS and retaining existing `{{VARIABLE}}` template syntax.

## Capabilities

### New Capabilities
- `node-matched-syscall-collection`: Match local machine identity to `_nodes` response and collect allowlisted syscall command output into the existing bundle with node-derived variables.

### Modified Capabilities
- `collection-execution`: Extend collection execution requirements to include `_nodes`-based local node matching, variable derivation, and non-fatal syscall execution behavior.

## Impact

- Target product: Elasticsearch.
- Affected area: core processing logic for `collect` execution path.
- Affected code likely includes collector orchestration, receiver raw-data plumbing, and host command execution modules under `src/processor/elasticsearch/`.
- Affected configuration: `assets/elasticsearch/syscalls.yml` parsing/selection by OS.
- Affected APIs: `_nodes` data consumption for local-node matching and variable extraction.
- No new CLI flags expected; existing `collect` UX remains unchanged.
