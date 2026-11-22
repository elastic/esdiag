---
type: Guide
title: Use ESDiag as an MCP server
description: Register the esdiag stdio MCP server with Claude Code, Codex, or another MCP client, and the tools it offers.
tags: [setup, mcp, agents]
---

# Use ESDiag as an MCP server

`esdiag mcp` serves ESDiag commands as Model Context Protocol (MCP) tools to a
local client such as Claude Code or Codex. The client starts the process and
talks to it over stdin and stdout. Each tool returns the same structured result
as the matching command's `--format json` output, reports progress for long
runs, and declares whether it reads, writes, or deletes configuration.

The server uses your normal ESDiag configuration: saved hosts, jobs, the saved
output, and the keystore. It needs no URLs, credentials, or environment
settings of its own.

## What you need

- The [native binary](installation.md#native-binary), built with the `mcp`
  feature. Release builds include it; `esdiag --help` lists `mcp` when it is
  available.
- Saved hosts for the clusters you want the agent to reach. See
  [Use ESDiag from a coding agent](agent-skills.md#save-your-clusters-as-hosts).

## Register the server

The ESDiag plugin for Claude Code and Codex registers the server for you. To
register it by hand, run one of these:

```sh
claude mcp add esdiag -- esdiag mcp
codex mcp add esdiag -- esdiag mcp
```

For other clients, add a stdio server that runs `esdiag` with the single
argument `mcp`:

```json
{
  "mcpServers": {
    "esdiag": {
      "command": "esdiag",
      "args": ["mcp"]
    }
  }
}
```

## Unlock the keystore first

The server never prompts for a password, because stdin carries the protocol.
Before you use saved hosts with keystore secrets, unlock the keystore in your
own terminal:

```sh
esdiag keystore unlock
```

While the keystore is locked and `ESDIAG_KEYSTORE_PASSWORD` is unset, tools
that need a secret fail with the `keystore_failed` category instead of waiting.
Unlock the keystore and retry the tool.

`host_add` and `host_update` accept only the name of an existing keystore
`secret`. Add credentials with `esdiag keystore add` in your terminal; never
paste them into the agent.

## Tools

| Tool | Command | Effect |
|---|---|---|
| `host_list` | `esdiag host list` | Read only |
| `host_auth` | `esdiag host auth` | Read only |
| `output_show` | `esdiag output show` | Read only |
| `keystore_status` | `esdiag keystore status` | Read only |
| `job_list` | `esdiag job list` | Read only |
| `process` | `esdiag process` | Writes diagnostic data |
| `collect` | `esdiag collect` | Collects from a cluster |
| `send` | `esdiag send` | Uploads an archive |
| `job_run` | `esdiag job run` | Runs a saved job |
| `agent_ask` | `esdiag agent ask` | Asks Agent Builder |
| `setup_output` | `esdiag setup output` | Installs assets on a deployment |
| `setup_elasticsearch` | `esdiag setup elasticsearch` | Installs assets on a deployment |
| `setup_kibana` | `esdiag setup kibana` | Installs assets on a deployment |
| `host_add` | `esdiag host add` | Changes configuration |
| `host_update` | `esdiag host update` | Changes configuration |
| `output_set` | `esdiag output set` | Changes configuration |
| `host_remove` | `esdiag host remove` | Deletes configuration |
| `job_delete` | `esdiag job delete` | Deletes configuration |
| `output_clear` | `esdiag output clear` | Deletes configuration |

The job tools need the `keystore` feature, `agent_ask` needs `agent`, and the
setup tools need `setup`. Release builds include all of them.

Tools reject output `-`, and `job_run` rejects saved jobs that export to
stdout, because stdout belongs to the protocol. Choose a directory or a saved
output host instead. Logs go to stderr; set `LOG_LEVEL` or pass
`esdiag --debug mcp` for more detail.

Commands that need a terminal or start long-lived services are not tools:
`serve`, `init`, `local`, `keystore add`, `keystore update`,
`keystore remove`, and `setup skills`, `completions`, `env`, and `context`.
