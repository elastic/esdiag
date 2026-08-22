## Context

The current portable skill uses Bash to resolve a second set of Agent Builder environment variables, call Kibana with `curl`, parse SSE with `jq` and `awk`, query Elasticsearch for diagnostic freshness, and persist a diagnostic-to-conversation map. This duplicates HTTP, authentication, configuration, and persistence behavior already owned by ESDiag and Agent Builder.

The actual runtime value is narrower: a user or agent should be able to submit a prompt to the Agent Builder agent attached to the same Kibana deployment where ESDiag indexed the diagnostic. The request must remain a real Kibana conversation so its history and follow-up UX are preserved. Distribution has a similarly narrow boundary: every binary built with the Cargo `agent` feature should be able to place its own exact portable skill into supported agent homes without downloading a plugin or finding a source checkout. ESDiag already has `KibanaClient` and `rust-embed`; `add-first-run-onboarding` defines canonical output-deployment resolution, and `standardize-cli-output` defines the finite YAML/JSON result boundary.

## Goals / Non-Goals

**Goals:**

- Provide one generic `esdiag agent ask <PROMPT>` operation.
- Use the output deployment's linked Kibana endpoint, space, and credentials.
- Reuse `KibanaClient` instead of building a parallel raw HTTP client.
- Consume Agent Builder SSE internally for progress and completion.
- Preserve every request and follow-up in Kibana Agent Builder history.
- Return a compact structured response with conversation and handoff information.
- Embed the canonical portable skill in every binary built with the Cargo `agent` feature and provide an offline, version-correct installer for supported local agents.
- Remove all shell helpers and external executable dependencies from the portable skill.

**Non-Goals:**

- An `esdiag agent converse` command, raw SSE proxy, normalized event stream, or interactive chat loop.
- Native diagnostic analysis, metrics, thresholds, or ES|QL reasoning.
- A native latest-diagnostic/freshness query.
- A native binding/connect command or an analysis-specific configuration layer.
- Persisting diagnostic-to-conversation mappings locally.
- Configuring Agent Builder agents, inference endpoints, connectors, licenses, or cluster assets.
- Automatically retrying an interrupted or failed model request.
- Downloading skills, installing marketplace plugins, modifying host configuration unrelated to skills, or managing arbitrary third-party skills.
- Silently overwriting an unrecognized or locally modified ESDiag skill directory.

## Decisions

### Expose one finite generic ask operation

The command shape is:

```text
esdiag agent ask [--agent <ID>] [--conversation <ID> | --new] <PROMPT>
```

The prompt is opaque application input. ESDiag does not require a diagnostic identifier flag and does not infer diagnostic selection. The portable skill can include the exact `diagnostic.id` returned by a structured process/job outcome, while broader questions can let the configured Agent Builder agent discover existing diagnostics with its own installed tools.

`esdiag process --ask <PROMPT>` is a convenience for a newly processed primary diagnostic: after successful processing, it starts a new generic ask whose prompt is exactly `diagnostic.id: <id>\n<PROMPT>`. It does not introduce diagnostic selection, local analysis, or conversation persistence. Because a document stream already owns stdout, `--ask` is incompatible with `process` output `-`.

Without `--conversation`, the command starts a new Kibana conversation; `--new` makes that intent explicit and conflicts with `--conversation`. A follow-up supplies the returned conversation ID. No ambient or persisted local conversation is selected.

Alternative considered: `esdiag analysis run --diagnostic ...`. That hard-codes one Agent Builder use case, requires local diagnostic-selection policy, and encourages ESDiag to duplicate reasoning already present in the agent.

### Resolve Agent Builder from the output deployment

The command asks the shared `OutputDeployment` resolver for the configured Kibana viewer and authentication. Environment-backed use requires `ESDIAG_OUTPUT_URL`, `ESDIAG_KIBANA_URL`, and the shared `ESDIAG_OUTPUT_*` authentication; saved use follows `ApplicationConfig.output` to its viewer host and referenced keystore secret.

The Kibana space comes from an explicit space segment in the viewer URL or the existing `ESDIAG_KIBANA_SPACE` override/default, normalized exactly once. The agent defaults to `elastic-ai-agent` and may be overridden by `--agent`; no inference endpoint is supplied because model routing belongs to the configured Agent Builder agent.

Alternative considered: preserve `ESDIAG_KIBANA_APIKEY`, `ESDIAG_ELASTICSEARCH_URL`, `ESDIAG_INFERENCE_ID`, and related plugin variables. They represent the same deployment twice and can drift from the destination that actually contains the diagnostic.

### Compose the existing KibanaClient

Implement Agent Builder request and SSE interpretation in a focused agent module that receives a configured `KibanaClient`. It uses `KibanaClient::request` or a narrowly extended streaming method, so TLS policy, base URL, authentication, and standard Kibana headers stay centralized. It does not construct another `reqwest::Client` or parse credentials itself.

No receiver, processor, or exporter trait changes are required. This is a CLI service over an existing product client, not a diagnostic processing stage.

### Consume SSE internally and return one finite outcome

The Agent Builder async conversation endpoint remains SSE. The command incrementally interprets conversation ID, reasoning, tool call, tool progress, tool result, completion, usage, and error events. Reasoning and tool progress are operational output on stderr. On completion, stdout receives one standard outcome:

```yaml
result: agent_response
conversation_id: 4f7b...
message: |
  The cluster is healthy overall...
kibana_url: https://kibana.example/s/esdiag/app/agent_builder/conversations/4f7b...
```

Optional safe usage metadata may be included. Relative links in returned markdown are resolved against the configured Kibana viewer URL. No raw SSE is exposed and no `converse` output category is added.

Before a request starts, ESDiag retrieves the selected agent's display name from Agent Builder through the same configured `KibanaClient`. Every stderr progress line uses that name as its prefix instead of the product-wide `Agent Builder` label. A failed name lookup is not a conversation failure: ESDiag falls back to a readable form of the explicitly selected agent ID.

Alternative considered: proxy SSE to stdout. It would create a second public streaming protocol, tie callers to Kibana event versions, and overlap with the deliberately excluded `converse` command.

### Keep Kibana as the only durable conversation store

Agent Builder persists the conversation. The result exposes its ID and a Kibana handoff link. Callers that want another turn pass `--conversation`; users can follow the link and continue in Kibana. ESDiag writes no conversation index, diagnostic mapping, prompt, or response file under `ESDIAG_HOME`.

This removes synchronization and privacy questions around a second local history while preserving the UX the user values.

### Fail safely after conversation creation

If the stream ends before completion after yielding a conversation ID, return a non-zero structured failure containing the safe ID, Kibana link when constructible, and `retry_safe: false`. Do not retry automatically. The existing Kibana conversation is the recovery location.

If no conversation ID was received, return a categorized safe HTTP/transport failure. Debug cause chains remain on stderr and secrets never enter the result.

### Delete the skill script layer only after all native boundaries exist

Map each helper to its owner:

| Current helper | Replacement |
|---|---|
| `extract-diagnostic.sh` | Structured collect, process, and job outcomes |
| `config.sh` | Canonical application/output deployment resolution |
| `connect.sh` | `esdiag init` endpoint validation and structured command failures during use |
| `latest-diagnostic.sh` | Agent Builder diagnostic discovery for existing data; exact native outcome ID for newly processed data |
| `analyze.sh` | `esdiag agent ask` |

After the structured-output, onboarding, and agent-ask changes are implemented, delete both canonical and generated `scripts/` directories and update the sync contract to keep them absent. The main skill composes native commands; `references/onboarding.md` handles first-run handoff.

### Embed one canonical script-free skill

Add a dedicated `RustEmbed` asset type rooted at `.agents/skills/esdiag/`. The embedded set includes `SKILL.md`, `references/**`, and supported `agents/**` metadata and excludes `scripts/**` by construction. The Cargo package already includes the canonical directory, so crates.io builds, Homebrew builds, release binaries, and local builds with the `agent` feature compile the same files into the executable.

The embedded bytes are the installation source of truth. Agent-enabled binaries report the running `CARGO_PKG_VERSION` and a deterministic content digest so installed state can be compared without a network lookup. The standalone plugin package remains a distribution option, but it is not required for binary users.

Alternative considered: download the skill from GitHub or the plugin marketplace. That introduces network, release-discovery, and source/version skew into a command whose strongest property is that the correct asset is already beside the code that knows how to use it.

### Detect supported user-scoped agent targets through adapters

The command shape is:

```text
esdiag agent skills [--target <claude|codex|opencode>]... [--force]
```

Small target adapters for Claude Code, Codex, and OpenCode know each host's documented user-scoped skill root, environment/path override where supported, and positive detection signals such as an existing agent home or executable. Default invocation installs to every detected target. Repeatable `--target` selection allows installation when an agent is installed but not detectable or its home has not yet been created.

The command never installs into the current repository implicitly. User scope is the correct default for a binary obtained through `cargo install` or Homebrew, and avoids modifying arbitrary projects. Detection is read-only until the complete plan has been validated.

Alternative considered: write one universal `~/.agents/skills` directory and rely on every host to discover it. Current hosts have distinct supported user locations, so an adapter boundary is more honest and testable than assuming a standard they do not all implement.

### Make installation atomic and ownership-safe

For each selected target, compare its existing `esdiag` directory with the embedded file manifest:

- Missing: stage and install the embedded skill.
- Exact match: report `unchanged` and perform no writes.
- Previously installed version with an intact ESDiag installation marker: stage an update, preserve a recoverable backup until replacement succeeds, and write the new marker last.
- Unrecognized or locally modified directory: report a conflict and do not overwrite unless the user supplies `--force`; forced replacement still preserves the original as a recoverable backup until the new installation succeeds.

The marker contains only installer identity, ESDiag version, and content digest; it contains no machine identifier, credential, or configuration. Paths are resolved and validated before mutation, replacements use a sibling temporary directory plus atomic rename where supported, and one target failure cannot be hidden by successes on other targets. A partial result lists every target action and exits non-zero when any selected target failed.

After installation or update, the outcome notes that already-running agent processes may require restart or reload before discovering the new skill.

Alternative considered: unconditionally replace the directory. Users may intentionally customize a skill, and package managers should not authorize destruction of unrelated user state.

### Let initialization compose the installer without owning it

`esdiag agent skills` owns target adapters, detection, preflight, conflict handling, installation, and structured per-target facts. `esdiag init` composes that same in-process service as an optional final stage after its required workflow configuration is valid; it does not spawn a second ESDiag process or reproduce installer decisions.

The initialization UI presents detected targets, permits explicit additional target selection, and permits decline. Declining completes initialization normally. A conflict or installation failure is returned as a per-target fact with the standalone recovery command, but does not invalidate the configured keystore, output deployment, hosts, or job. No installation preference or target path is persisted in `esdiag.yml`; installed files and their ownership markers remain authoritative.

Alternative considered: have onboarding own its own installer. That reverses the dependency, duplicates a security-sensitive mutation workflow, and prevents `agent skills` from being the standalone recovery path.

## Risks / Trade-offs

- **Agent Builder SSE changes across Kibana versions** → Test recorded fixtures for every consumed event and treat unknown events as ignorable unless completion becomes impossible.
- **No local automatic conversation reuse** → Return the conversation ID prominently; agent callers retain it in context and humans use the Kibana history link.
- **Agent discovery of existing diagnostics may cost more than direct metadata ES|QL** → Accept this because diagnostic discovery belongs to the configured agent and avoids a second analytical/query policy in ESDiag.
- **Interrupted requests may already incur inference cost** → Never retry automatically after a conversation ID and return `retry_safe: false` with the Kibana recovery link.
- **Viewer configuration may be incomplete** → Fail through canonical output-deployment validation and direct users to `esdiag init`; do not introduce fallback URLs or credentials.
- **Agent output is prose controlled by cluster assets** → Relay it without parsing it into local decisions; only SSE envelope metadata becomes typed fields.
- **Agent installation paths evolve independently** → Isolate paths and detection in per-host adapters and cover documented overrides and fixture homes.
- **An existing skill may be locally customized** → Compare digests, recognize only installer-owned state automatically, and require explicit force for conflicts.
- **Multi-target installation can partially succeed** → Preflight every target, report per-target results, exit non-zero on any failure, and make each replacement independently atomic and recoverable.
- **Embedding increases binary size** → Embed only the script-free portable skill and metadata; references are small relative to existing embedded web and setup assets.

## Migration Plan

1. Depend on the standard structured outcome writer and canonical output-deployment resolver.
2. Add focused Agent Builder request/event types and recorded SSE fixtures around `KibanaClient`.
3. Delete canonical and generated `scripts/` directories and define the canonical embedded skill asset set.
4. Add host target adapters, detection, manifest/digest comparison, atomic installation, conflict handling, and structured `esdiag agent skills` results.
5. Add `esdiag agent ask`, explicit conversation continuation, progress routing, and safe failure outcomes.
6. Update the canonical skill to pass exact structured diagnostic IDs when newly available and use Agent Builder for existing-diagnostic discovery.
7. Remove analysis-specific variables and shell-helper instructions from skill references and plugin documentation.
8. Regenerate package assets, replace shell integration fixtures with native ask/installer tests, and verify the skill is present in `cargo package` inputs and built binaries.
9. Update CLI documentation and changelog.

Rollback can restore the shell helpers temporarily without changing cluster conversations or local data because the native command adds no persisted conversation state.

## Open Questions

None. `converse` and every form of public conversation stream are explicitly excluded.
