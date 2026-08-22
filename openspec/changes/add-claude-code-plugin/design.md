## Context

ESDiag has two agent-facing surfaces. `.agents/skills/esdiag/` teaches a local coding agent to collect and process diagnostics, while `esdiag setup` installs dashboards, tools, workflows, and the `agentic-diagnostic-assistant` skill into Kibana and attaches it to `elastic-ai-agent`. The analytical knowledge belongs in the cluster; copying it into every host integration would drift and would move substantial inference cost to the user's local model quota.

Production verification shows the configured Agent Builder agent carries the diagnostic skill and generic tools such as `platform.core.execute_esql` and `user_diagnostic_id_fetcher`. A full analysis measured roughly 109,600 cluster input tokens and 3,600 output tokens while returning about 1,100 tokens of markdown. The portable skill should therefore orchestrate ESDiag and relay the cluster agent's response, not reimplement analysis.

The first implementation proved the user experience with shell helpers, but also exposed the wrong architectural boundaries: plugin-specific URLs and credentials duplicate the processed-diagnostic output deployment; direct freshness ES|QL duplicates diagnostic discovery already available to Agent Builder; local conversation mapping duplicates Kibana history; and first-job setup in the normal skill conflates daily use with new-user onboarding.

This design records the durable plugin contract. Structured results, general onboarding/configuration, and native Agent Builder transport are implemented by the successor `standardize-cli-output`, `add-first-run-onboarding`, and `add-agent-cli` changes before this change is archived.

## Goals / Non-Goals

**Goals:**

- Distribute one portable ESDiag skill to Claude Code, Codex, and OpenCode through thin host adapters.
- Keep analytical knowledge and inference spend on the configured Kibana Agent Builder deployment.
- Preserve progressive status during long analyses and preserve every real request in Kibana conversation history.
- Use exact diagnostic identifiers from ESDiag outcomes when new diagnostics are created.
- Keep normal skill use, local first-run onboarding, and cluster provisioning as distinct workflows.
- Keep plugin packaging independent of application URLs, credentials, job preferences, and conversation persistence.

**Non-Goals:**

- Reproducing Agent Builder analysis, ES|QL thresholds, or recommendations locally.
- A host-plugin-specific HTTP client, direct Elasticsearch freshness query, or configuration file.
- An `esdiag agent converse` command, raw event proxy, or interactive terminal chat.
- Configuring licenses, Agent Builder assets, inference services, connectors, or model credentials during plugin installation.
- Treating unstructured agent prose as a local control protocol.

## Verified Constraints

### Agent Builder MCP exposes tools, not agents or skills

`/api/agent_builder/mcp` exposes the tool registry; it does not expose an `invoke_skill` or `ask_agent` operation. An MCP-only integration would require copying diagnostic knowledge locally, defeating the design.

### Workflow agent selection is not runtime-templatable

Kibana Workflows can invoke an agent with `ai.agent`, but the top-level `agent-id` and `inference-id` fields are not template-expanded. A workflow tool cannot support caller-selected agents without provisioning one workflow per choice.

### The chat endpoint returns prose

The Agent Builder chat endpoint rejects a response `schema`. The completed message is unstructured markdown. ADA already supplies presentation rules, so clients relay it and do not parse it into actions.

### The asynchronous chat endpoint provides useful progress

`POST /s/{space}/api/agent_builder/converse/async` emits conversation, reasoning, tool, completion, and usage events. A native client can consume these internally and render progress without exposing a public raw-event command.

### Tools execute with the caller's privileges

Agent Builder route authorization alone is insufficient. The diagnostic tools query Elasticsearch as the current user, so the resolved output credential needs Kibana Agent Builder access plus read and view-index-metadata access to ESDiag data streams. The same output deployment credential should be validated against both Elasticsearch and Kibana rather than creating a second plugin credential.

### Model availability is proven only by a real request

Kibana connectors do not enumerate Elastic Inference Service models, and Elasticsearch inference listings do not prove what a selected Agent Builder agent can use. The first real question is the authoritative model check. A missing model is attributed to deployment provisioning without issuing a throwaway conversation.

### Cluster inference cost grows with conversation depth

Measured requests showed an approximately 8,500-token input floor and about 109,600 input tokens for a full diagnostic analysis. Follow-ups replay history and increase cluster cost. Clients surface safe usage data when available and never retry paid requests automatically after conversation creation.

## Decisions

### Package one canonical Open Agent Skill

`.agents/skills/esdiag/` is the source of truth. Claude Code and Codex add only package metadata; OpenCode discovers the same portable skill. A deterministic sync step generates package contents and drift checks fail release validation when a host package diverges. The successor `add-agent-cli` change also embeds this script-free canonical source in the binary and exposes it through `esdiag agent skills`, giving `cargo install`, Homebrew, and other binary users an offline version-matched installation path without making the standalone plugin package mandatory.

Alternative considered: maintain host-specific skills. That would multiply documentation, workflow, and security review surfaces without adding capability.

### Put Agent Builder transport behind native ESDiag commands

The durable transport is `esdiag agent ask`, defined by `add-agent-cli`. It reuses `KibanaClient` and the canonical output deployment, consumes the Agent Builder asynchronous response internally, writes progress to stderr, and returns one structured terminal outcome. The plugin calls that command instead of owning `curl`, SSE parsing, configuration, or local state.

This is intentionally an `ask` operation only. An `agent converse` subcommand, public SSE/NDJSON event proxy, and interactive chat loop are excluded.

Alternative considered: keep shell transport in the skill. It adds runtime dependencies, duplicates authentication behavior, and makes protocol correctness part of prompt packaging.

### Resolve one output deployment

`ESDIAG_OUTPUT_*` and `ESDIAG_KIBANA_URL` identify the Elasticsearch destination and its attached Kibana instance. For persistent local use, `esdiag.yml` selects a send-role Elasticsearch host whose `viewer` selects its view-role Kibana host; both may reference the same encrypted secret.

The plugin introduces no `ESDIAG_ELASTICSEARCH_URL`, Kibana-only API key, API-key file, inference route, freshness window, saved-job default, or separate configuration resolver. Agent Builder runs where the processed data lands.

Alternative considered: retain plugin settings. That permits analysis to target a different deployment from processing and produces two credential stores for one logical destination.

### Make agent selection an explicit command concern

`elastic-ai-agent` remains the default because `esdiag setup` attaches the diagnostic skill there. Operators may select another agent explicitly with the native command option. Inference selection is not exposed; the configured Agent Builder agent owns its model routing.

### Relay analysis without re-deriving it

The client presents the completed Agent Builder markdown, resolving relative Kibana links against the configured viewer. It does not rerun ES|QL, recompute metrics, infer severity, trigger remediation, or decide whether to collect based on parsing prose.

### Keep Kibana as the conversation store

Every real question uses the Agent Builder chat API. The result contains the conversation identifier and Kibana handoff link. Follow-ups explicitly pass the identifier; ESDiag and the plugin persist no second diagnostic-to-conversation map, prompt history, or response history.

If a request is interrupted after conversation creation, the client returns the safe identifier and marks retry unsafe. The user resumes from Kibana or explicitly continues the same conversation.

### Use exact structured identity for newly processed diagnostics

Structured collect, process, and saved-job outcomes expose terminal facts directly. When processing returns `diagnostic.id`, the skill includes that exact value in its Agent Builder prompt. It never scrapes completion prose or runs a latest-diagnostic query to rediscover work it just created.

When no new or explicit identifier exists, the configured Agent Builder agent uses its own installed tools to discover appropriate existing diagnostics. ESDiag does not own a freshness threshold or duplicate selection ES|QL.

### Separate collection authorization from discovery

Explicit requests to collect or run a new diagnostic authorize the configured saved workflow. References to an explicit or just-created diagnostic never collect. A general health question first goes to Agent Builder for existing-data discovery; live collection still requires an explicit request or subsequent approval.

This preserves the safety boundary around production API calls without maintaining a local intent classifier plus freshness query.

### Move first-run setup to `esdiag init`

Keystore password selection, default identity, output Elasticsearch/Kibana pairing, collection hosts, and the first saved job form a general new-user workflow. They belong to the terminal-native `esdiag init` state machine and `references/onboarding.md`, not routine skill orchestration.

The skill hands uninitialized users to onboarding and never asks for credentials in conversation. A configured user goes directly to native commands without repeated setup offers.

### Preserve saved-job terminal facts

Saved jobs compose `Collect` or `Load` input with optional `Save`, `Process { export }`, and `Send` stages. The unified executor outcome exposes every durable result that survives the run: a retained archive, processed diagnostic identifier and Kibana link, and/or upload destination. These facts may coexist and none is reduced to a mutually exclusive primary variant. This is a CLI contract useful beyond the plugin and is not worked around in skill code.

## Risks / Trade-offs

- **Model availability remains a deployment prerequisite** → Attribute failure on the first real request; never automate billing, connectors, or inference credentials.
- **Agent responses remain unstructured prose** → Relay markdown and treat only the transport envelope as typed data.
- **Cluster spend grows with conversation depth** → Surface usage when available, document cost, and prohibit automatic retry after conversation creation.
- **Agent discovery of existing diagnostics costs inference** → Accept the cost to keep discovery policy with the configured agent and avoid duplicate ES|QL behavior.
- **No automatic local follow-up mapping** → Return conversation identifiers and Kibana links prominently; callers explicitly continue them.
- **Bundled-skill drift** → Generate packages from the canonical directory and validate exact synchronization.
- **Successor changes must land before archive** → Keep this change open until structured output, onboarding, native `agent ask`, and script removal satisfy the revised contract.

## Migration Plan

1. Retain the portable package and saved-job result work already completed by this change.
2. Implement `standardize-cli-output` and remove completion-prose parsing.
3. Implement `add-first-run-onboarding`, migrate application settings, and add the onboarding reference.
4. Implement `add-agent-cli`, replace shell transport with `esdiag agent ask`, embed the canonical skill for `esdiag agent skills`, and delete skill script directories.
5. Regenerate Claude Code, Codex, and OpenCode packages and verify the same native workflow in each.
6. Revalidate this change against its revised specs before archive.

## Open Questions

None. `converse` and every public Agent Builder event-stream command are explicitly excluded.
