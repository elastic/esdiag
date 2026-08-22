## Why

The portable ESDiag Agent Skill currently implements Agent Builder HTTP transport, SSE parsing, configuration, conversation persistence, and metadata queries in Bash, while users who install only the binary have no direct way to install the matching skill. ESDiag can own both boundaries natively: `agent ask` reuses output-deployment resolution and `KibanaClient`, and `agent skills` installs the exact script-free skill embedded in that binary.

## What Changes

- Add `esdiag agent ask <PROMPT>` as a generic one-turn Agent Builder client; diagnostic selection and analysis remain prompt content interpreted by the configured cluster-side agent.
- Resolve Kibana, authentication, and space from the same output deployment used for processed diagnostics. Do not add `ESDIAG_ELASTICSEARCH_URL`, `ESDIAG_KIBANA_APIKEY`, `ESDIAG_KIBANA_APIKEY_FILE`, or an `AnalysisConfig` layer.
- Reuse the existing `KibanaClient` for the Agent Builder request and consume its SSE response internally.
- Emit reasoning and tool progress on stderr and one structured response outcome on stdout containing the completed message, conversation ID, and Kibana link.
- Support explicit follow-up with `--conversation <ID>` and explicit new conversations without maintaining a second local diagnostic-to-conversation state file.
- Preserve every real request in Kibana Agent Builder history so a user can follow the returned link and continue there.
- Add `esdiag agent skills` to detect supported local agent installations and install or update the user-scoped ESDiag skill from version-matched `rust_embed` assets without network access.
- Install atomically, report detected targets and actions structurally, leave matching installations unchanged, and protect unrecognized or locally modified skill directories from implicit overwrite.
- Remove the portable skill's remaining configuration, binding, freshness, and analysis shell helpers; structured collect/process/job outcomes provide newly created diagnostic identifiers, while Agent Builder handles discovery and reasoning over existing diagnostics.
- Keep an Agent Builder conversation-stream proxy, interactive chat loop, `converse` subcommand, native latest-diagnostic query, and native binding command out of scope.

## Capabilities

### New Capabilities

- `cli-agent-ask`: Generic Agent Builder question submission, internal SSE consumption, structured completion, explicit follow-up, failure safety, and Kibana history handoff.
- `cli-agent-skills`: Offline detection, inspection, installation, and safe version-correct updates of the binary-embedded portable ESDiag skill for supported coding agents.

### Modified Capabilities


## Impact

- **Target Elastic product:** Kibana Agent Builder in the Kibana instance attached to the configured Elasticsearch output deployment.
- **CLI:** Adds finite `esdiag agent ask` and `esdiag agent skills` commands. Both follow the standard YAML/JSON outcome contract when `standardize-cli-output` is implemented.
- **Client layer:** Extends or composes the existing `KibanaClient`; no parallel raw `reqwest` configuration client is introduced.
- **Configuration:** Depends on canonical output-deployment resolution from `add-first-run-onboarding`; command-level agent and conversation overrides remain explicit.
- **Core processing:** No analytical logic, ES|QL freshness query, diagnostic transformation, or collection behavior changes.
- **Agent assets:** Removes all canonical and generated skill scripts, embeds `SKILL.md`, `references/`, and supported agent metadata from the canonical skill into the binary, and replaces helper execution with native commands.
- **Packaging:** `cargo install`, Homebrew, and other binary distribution channels carry the same version-matched skill without requiring a repository checkout or plugin marketplace download.
