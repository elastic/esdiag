## Why

ESDiag already publishes two halves of an agent-driven diagnostic workflow that do not meet: a portable operations skill drives the CLI, while `esdiag setup` installs diagnostic expertise into a Kibana Agent Builder agent. Packaging the operations skill for common coding agents lets the local agent orchestrate collection and present cluster-side analysis without copying analytical knowledge or inference cost into every host integration.

## What Changes

- Add one portable ESDiag Agent Skill single-sourced from `.agents/skills/esdiag/`, with thin Claude Code and Codex package metadata and direct OpenCode discovery.
- Make that same canonical skill available through the version-matched, binary-embedded `esdiag agent skills` installer owned by the successor `add-agent-cli` change, so plugin or marketplace installation is optional for binary users.
- Teach the skill to obtain exact diagnostic identifiers from ESDiag command results and delegate diagnostic reasoning to the output deployment's Agent Builder agent.
- Preserve real Kibana Agent Builder conversations and return links so users can continue analysis in Kibana.
- Keep host packaging independent of ESDiag application configuration, credential storage, first-run onboarding, and cluster provisioning.
- Route newly installed or unconfigured users to an ESDiag-owned onboarding workflow rather than teaching the skill to collect credentials or construct persistent state.
- Treat the Agent Builder transport as an ESDiag CLI responsibility rather than a host-plugin-specific HTTP, SSE, or query implementation.
- Update saved-job reporting so a phase-composed run identifies every durable diagnostic, retained archive, and upload destination it produced.

## Capabilities

### New Capabilities

- `claude-code-plugin`: Portable skill distribution, installation, single-sourcing, thin host adapters, and separation from application onboarding and cluster provisioning.
- `agent-builder-analysis`: Delegated cluster-side analysis, progress presentation, exact diagnostic handoff, Kibana conversation continuity, and safe failure handling.

### Modified Capabilities

- `saved-jobs`: `esdiag job run` reports every durable diagnostic identifier, retained archive path, and upload destination produced by its selected stages so callers can consume the terminal result.

## Impact

- **Target Elastic products:** Kibana Agent Builder in the Kibana instance attached to the processed-diagnostic Elasticsearch output deployment. Elasticsearch, Logstash, and Kibana diagnostics remain unchanged as analyzed subject matter.
- **Rust CLI:** Saved-job result reporting changes here. General structured outcomes, first-run configuration, native Agent Builder transport, and binary-embedded skill installation are defined by the successor `standardize-cli-output`, `add-first-run-onboarding`, and `add-agent-cli` changes.
- **Web UI/Core processing:** No behavioral changes beyond shared saved-job terminal facts.
- **Kibana assets:** No new workflow or tool assets. The skill uses the diagnostic assets installed by `esdiag setup` and does not configure model access.
- **Repository surface:** Adds portable skill assets plus Claude Code and Codex adapters generated from the canonical source for Claude Code, Codex, and OpenCode.
- **Configuration and credentials:** The plugin defines no independent URLs, API keys, inference routing, saved-job defaults, or conversation state. ESDiag owns persistent preferences, hosts, and encrypted credentials.
- **Cost attribution:** Diagnostic reasoning remains on the cluster inference connector; local agents orchestrate and present results.
- **Documentation:** Separates routine skill use, secure local onboarding, and cluster provisioning.
