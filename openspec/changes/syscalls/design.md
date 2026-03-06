## Context

The `collect` workflow already gathers Elasticsearch API diagnostics and packages them into one archive bundle. Host syscall command data exists as a repository-managed command inventory (`assets/elasticsearch/syscalls.yml`), but collection is not currently wired to the Java implementation strategy that first resolves node context from `_nodes`. The requested behavior is to keep user workflow unchanged while deriving syscall variables from matched `_nodes` entry and local machine identity before command execution. Constraints remain cross-platform support and non-fatal command handling.

## Goals / Non-Goals

**Goals:**
- Match the local machine to one node in `_nodes` using host/network identity gathered locally.
- Resolve `PID` and `LOGPATH` from matched `_nodes` node data.
- Resolve `JAVA_HOME` by inspecting the process command line for the resolved PID.
- Execute syscall commands from `assets/elasticsearch/syscalls.yml` for the active host OS using resolved variables.
- Add syscall outputs to the existing API diagnostic bundle without introducing new CLI flags.
- Keep syscall command failures/timeouts non-fatal and surfaced as warnings.

**Non-Goals:**
- Adding a new user-facing `--syscalls` flag or separate syscall-only command.
- Executing arbitrary user-provided shell commands outside repository-managed allowlist.
- Changing behavior for remote-only collection targets where local Elasticsearch is not present.

## Decisions

1. Resolve local node context via `_nodes` matching before syscall execution.
Rationale: Mirrors proven Java behavior and provides authoritative node metadata (`path.logs`, `process.id`) for template variables.
Alternative considered: process-only local detection first. Rejected because it bypasses cluster node context and may pick incorrect values.

1a. Perform local identity collection for matching using hostname, interface display names, interface IP addresses, interface hostnames, and canonical hostnames.
Rationale: Multiple host identifiers are needed for robust mapping in containerized and cloud environments.
Alternative considered: hostname-only matching. Rejected due to frequent mismatch in orchestrated environments.

1b. Match local machine identity against each node's `host` and `ip` values in `_nodes`.
Rationale: The user-specified contract is to compare locally gathered IP/hostname identity with `_nodes.host` and `_nodes.ip`.
Alternative considered: matching by node name only. Rejected because names can be deployment-specific aliases.

1c. Resolve tie-breaks using first-match order.
Rationale: User-selected behavior is deterministic first-match order when more than one node qualifies.
Alternative considered: scoring, process-id ordering, or lexicographic node-id ordering. Rejected to maintain parity with requested behavior.

2. Use `assets/elasticsearch/syscalls.yml` as the single source of command definitions for Linux/macOS/Windows.
Rationale: Existing file already captures per-OS command mapping and keeps command surface explicit.
Alternative considered: hardcoded commands in Rust source. Rejected due to maintainability and duplication.

3. Integrate syscall execution into the existing collector pipeline and archive exporter.
Rationale: Ensures one unified bundle and avoids introducing a second packaging flow.
Alternative considered: separate output artifact. Rejected because requirement is inclusion in existing bundle.

4. Apply bounded execution and warning-only failure semantics per syscall command.
Rationale: Maintains diagnostic progress when individual commands fail or timeout.
Alternative considered: fail-fast on first command error. Rejected because user requested non-fatal behavior.

5. Infer `LOGPATH` and `PID` from matched `_nodes` data, and infer `JAVA_HOME` from `ps -ef` output for that PID.
Rationale: `_nodes` gives authoritative node runtime settings while `JAVA_HOME` comes from actual process command line and bundled JRE path.
Alternative considered: derive all variables from `ps -ef` only. Rejected because `path.logs` is already explicitly available in `_nodes`.

5a. Implement syscall collection through receiver raw-data interfaces rather than parsed data-source flow.
Rationale: Syscall command output is pure text collection and should use raw capture semantics.
Alternative considered: adding parsing models for syscall outputs. Rejected because this change is collection-only.

5b. Add a distinct path discriminator for syscall command execution (for example `PathType::SystemCall`).
Rationale: Receiver routing must clearly separate command execution from file-path reads and URL fetches.
Alternative considered: overloading existing `PathType::File` or `PathType::Url`. Rejected because it blurs source semantics.

6. Keep `assets/elasticsearch/syscalls.yml` shape and `{{VARIABLE}}` templating unchanged.
Rationale: Existing configuration is already in use and readable.
Alternative considered: introducing a new template syntax or metadata schema. Rejected to avoid migration churn.

## Risks / Trade-offs

- [Incorrect node match from local identity ambiguity] -> Mitigation: deterministic multi-signal matching with explicit tie-break order and warning when ambiguous.
- [Platform command availability differences] -> Mitigation: treat missing commands as warning-only outcomes.
- [Long-running commands increasing collect duration] -> Mitigation: enforce per-command timeout and continue-on-timeout behavior.
- [Security surface from shell invocation] -> Mitigation: execute only allowlisted commands from repository-managed config.

## Migration Plan

- Implement `_nodes`-matching and syscall execution wiring in the existing collector path.
- Keep archive format backward-compatible by adding syscall files in deterministic paths.
- Validate behavior with cross-platform-oriented tests and warning-path assertions.
- Rollback strategy: disable syscall execution branch while leaving API collection unchanged.

## Open Questions

- Should syscall command stdout/stderr be stored in a single combined artifact per command, or split into separate files for easier post-analysis?
