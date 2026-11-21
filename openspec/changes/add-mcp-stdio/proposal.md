## Why

Coding agents drive ESDiag today by shelling out to the CLI and parsing its YAML. A local Model Context Protocol (MCP) server gives them typed tool schemas, structured results, progress for long runs, and per-tool safety hints, and lets the ESDiag plugin register ESDiag tools without any shell instructions.

## What Changes

- Add an `mcp` cargo feature, included in `full`, that builds an MCP tool catalog in `esdiag-core` and an `esdiag mcp` subcommand in the `esdiag` binary that serves it over stdio.
- Expose these tools, each backed by the same library operation as its CLI command and returning the same typed outcome as `--format json`:
  - Read-only discovery: `host_list`, `host_auth`, `job_list`, `output_show`, `keystore_status`.
  - Core workflow: `job_run`, `process`, `collect`, `agent_ask`.
  - Configuration and delivery: `send`, `setup_output`, `setup_elasticsearch`, `setup_kibana`, `host_add`, `host_update`, `output_set`.
  - Destructive: `host_remove`, `job_delete`, `output_clear`.
- Report the ESDiag version in the MCP server information rather than as a tool.
- Accept keystore secret references, never raw credentials, in tool parameters. Report a locked keystore as a structured tool failure instead of prompting.
- Reserve stdout for the MCP protocol: logs go to stderr, and tools reject stdout (`-`) as an output target.
- Report progress for `collect`, `process`, `job_run`, and `send` as MCP progress notifications.
- Register the server in the Claude Code and Codex plugin packages so installing the plugin enables the ESDiag tools.
- Not exposed: `serve`, `init`, `local`, `keystore add/update/remove`, `setup skills`, `setup completions`, `setup env`, `setup context`, and the `--sources` override.

Prerequisites, in stack order:

1. `extract-gui-crate` ([#467](https://github.com/elastic/esdiag/pull/467)), which moves the library to `esdiag-core` and the server to `esdiag-gui`.
2. `extract-command-operations`, which moves command bodies from `main.rs` into `esdiag-core::operations` functions that return typed outcomes without printing or prompting. That refactor changes no behavior.

`add-mcp-http` stacks on this change and serves the same catalog from `esdiag serve`.

## Capabilities

### New Capabilities

- `mcp-tool-catalog`: The transport-independent MCP tool set: names, parameters, results, failures, safety annotations, credential handling, feature gating, and progress.
- `mcp-stdio-server`: The `esdiag mcp` subcommand: stdio transport, stdout ownership, logging, lifecycle, and the `mcp` cargo feature.

### Modified Capabilities

- `claude-code-plugin`: The plugin packages register the ESDiag stdio MCP server.
- `workspace-crate-architecture`: The preserved cargo feature list gains `mcp`, which `full` includes and which builds without `esdiag-gui`.

## Impact

- Target products: Elasticsearch, Kibana, Logstash, and ECK diagnostics, through the existing collect, process, send, and setup paths; Kibana Agent Builder through `agent_ask`. Processing output is unchanged.
- Surfaces: CLI (new `esdiag mcp` subcommand) and core library (new `mcp` module in `esdiag-core`). No Web UI change.
- Code: `crates/esdiag-core/src/mcp/` (catalog), root `src/main.rs` (subcommand and stdio wiring), `plugin/` manifests.
- Dependencies: `rmcp` (features `server`, `macros`) in `esdiag-core`; `rmcp` with `transport-io` in the root package; `schemars` through `rmcp`.
- Features: root `mcp` forwards to `esdiag-core/mcp`; `full` gains `mcp`.
- Docs: `docs/` MCP usage page, `.agents/skills/esdiag/` guidance on preferring MCP tools when available, `CHANGELOG.md`.
