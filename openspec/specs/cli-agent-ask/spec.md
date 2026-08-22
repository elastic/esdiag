## Purpose

Define native Agent Builder CLI operations for finite questions, diagnostic follow-up, canonical output deployment resolution, and safe structured outcomes.

## Requirements

### Requirement: Generic Agent Builder Ask Command
The CLI SHALL provide `esdiag agent ask <PROMPT>` as a finite operation that submits the prompt to a configured Kibana Agent Builder agent. The prompt SHALL remain opaque to ESDiag, and the command MUST NOT require a diagnostic identifier or implement diagnostic reasoning locally.

#### Scenario: New question starts a Kibana conversation
- **WHEN** the user runs `esdiag agent ask "Analyze diagnostic abc123"` without a conversation identifier
- **THEN** the command sends the prompt to the configured Agent Builder agent
- **AND** starts a conversation persisted in Kibana history
- **AND** returns the completed agent message and conversation ID

#### Scenario: Explicit agent override
- **GIVEN** the output deployment contains more than one Agent Builder agent
- **WHEN** the user supplies `--agent custom-agent`
- **THEN** the request targets `custom-agent`
- **AND** omission of `--agent` targets `elastic-ai-agent`

### Requirement: Agent CLI Capability Is Feature-Gated
Agent Builder and coding-agent CLI capabilities SHALL require the Cargo `agent` feature. The default `full` feature set SHALL include `agent`. Builds without `agent` MUST omit `esdiag agent` and the Agent Builder-specific `esdiag process --ask` option from command parsing and help.

#### Scenario: Minimal build omits Agent CLI commands
- **GIVEN** ESDiag is built with `--no-default-features`
- **WHEN** a user views CLI help or invokes `esdiag agent ask <PROMPT>`
- **THEN** the `agent` command is absent
- **AND** the invocation is rejected as an unknown command
- **AND** `esdiag process --help` does not contain `--ask`

#### Scenario: Full build retains Agent CLI commands
- **GIVEN** ESDiag is built with its default `full` feature set
- **WHEN** a user views CLI help
- **THEN** `agent ask`, `agent skills`, and `process --ask` are available

### Requirement: Process Can Ask About Its New Diagnostic
The CLI SHALL accept `esdiag process ... --ask <PROMPT>`. After processing successfully produces a diagnostic identifier, it SHALL start a new Agent Builder conversation with a prompt exactly equivalent to:

```text
diagnostic.id: <diagnostic-id>
<PROMPT>
```

This request SHALL otherwise follow `esdiag agent ask` behavior, including canonical output-deployment resolution, the default agent, finite structured response handling, and safe failure behavior. ESDiag MUST NOT submit an Agent Builder request when processing fails or produces no diagnostic identifier. Because processed document streams own stdout, `--ask` MUST reject `process` output `-` before processing begins.

#### Scenario: Process asks about its completed diagnostic
- **WHEN** `esdiag process <INPUT> --ask "What is the highest-risk finding?"` successfully produces diagnostic ID `prod-es@2026-08-18~a1b2`
- **THEN** it starts a new Agent Builder conversation with the prompt `diagnostic.id: prod-es@2026-08-18~a1b2\nWhat is the highest-risk finding?`
- **AND** the request uses the default `elastic-ai-agent` unless that default changes for `agent ask`
- **AND** its completed response follows the standard finite Agent Builder outcome contract

#### Scenario: Failed processing does not ask Agent Builder
- **WHEN** `esdiag process <INPUT> --ask <PROMPT>` fails before producing a diagnostic identifier
- **THEN** it does not submit an Agent Builder request

#### Scenario: Process document streaming cannot ask Agent Builder
- **WHEN** the user runs `esdiag process <INPUT> - --ask <PROMPT>`
- **THEN** the command rejects the incompatible options before processing begins
- **AND** it does not submit an Agent Builder request

### Requirement: Agent Ask Uses Canonical Output Deployment
The command SHALL obtain Kibana URL, space, and authentication from the canonical processed-diagnostic output deployment. It MUST NOT resolve a separate analysis Elasticsearch URL, Kibana API key, Kibana API-key file, inference endpoint, saved job, or freshness-window configuration.

#### Scenario: Saved output selects Kibana viewer
- **GIVEN** application configuration selects an Elasticsearch send host linked to a Kibana view host
- **WHEN** `esdiag agent ask` resolves its client
- **THEN** it uses the linked Kibana host and its resolved authentication
- **AND** the request is scoped to that host's configured space exactly once

#### Scenario: Environment output shares authentication
- **GIVEN** an environment-backed output deployment defines `ESDIAG_OUTPUT_URL`, `ESDIAG_KIBANA_URL`, and `ESDIAG_OUTPUT_APIKEY`
- **WHEN** the command constructs its Kibana client
- **THEN** it authenticates Kibana with `ESDIAG_OUTPUT_APIKEY`
- **AND** requires no Kibana-specific credential variable

### Requirement: Existing KibanaClient Owns Transport
Agent Builder requests SHALL use the existing `KibanaClient` authentication, URL, TLS, and request behavior. The implementation MUST NOT construct a parallel raw HTTP client with separate configuration and MUST NOT invoke `curl`, Bash, or another executable.

#### Scenario: Native request has no helper dependency
- **GIVEN** `esdiag` is installed without `curl`, `jq`, `grep`, `sed`, `awk`, or Bash
- **WHEN** the user runs `esdiag agent ask`
- **THEN** the request and SSE response are handled in process
- **AND** the only executable runtime dependency is `esdiag`

### Requirement: Internal SSE Produces A Finite Structured Outcome
The command SHALL consume Agent Builder SSE internally, render reasoning and tool progress on stderr, and emit one standard YAML response outcome on stdout after completion, or the equivalent compact JSON when requested. It MUST NOT expose raw SSE or add an `agent converse` command or other public conversation stream.

#### Scenario: Completed SSE returns one outcome
- **WHEN** Agent Builder emits a conversation ID, progress events, and a completed message
- **THEN** progress appears only on stderr
- **AND** stdout contains exactly one typed response with message, conversation ID, and Kibana conversation link
- **AND** no SSE framing appears on stdout

### Requirement: Agent Progress Identifies The Selected Agent
Before rendering Agent Builder reasoning or tool-progress updates, the CLI SHALL resolve the selected agent's display name from the configured Kibana Agent Builder deployment and prefix every such stderr line with `<agent-name>:`. Failure to obtain that display name MUST NOT prevent the conversation request; in that case, the CLI SHALL use a readable rendering of the explicit agent identifier as the prefix.

#### Scenario: Progress uses the configured agent name
- **GIVEN** agent ID `diagnostic-agent` has Agent Builder name `Diagnostic Agent`
- **WHEN** `esdiag agent ask` receives a reasoning or tool-progress event
- **THEN** it writes the update to stderr with the prefix `Diagnostic Agent:`
- **AND** it does not prefix the update with `Agent Builder:`

#### Scenario: Relative message links are resolved
- **GIVEN** the completed Agent Builder markdown contains a relative Kibana link
- **WHEN** the response outcome is created
- **THEN** the link is resolved against the configured Kibana viewer URL
- **AND** no unrelated Elasticsearch or Kibana base is used

### Requirement: Follow-Up Is Explicit And Kibana Remains Authoritative
The command SHALL continue a conversation only when the caller supplies `--conversation <ID>`. An invocation without that option SHALL start a new conversation, and `--new` SHALL explicitly request the same new-conversation behavior while conflicting with `--conversation`. ESDiag MUST NOT persist a local conversation map, prompt history, or response history.

#### Scenario: Explicit follow-up continues conversation
- **GIVEN** a prior ask returned conversation ID `conv-1`
- **WHEN** the user runs `esdiag agent ask --conversation conv-1 "Explain further"`
- **THEN** the request continues `conv-1` in Kibana
- **AND** the resulting turn remains visible in Kibana history

#### Scenario: New ask does not consult local history
- **GIVEN** prior conversations exist in Kibana
- **WHEN** the user runs an ask without `--conversation`
- **THEN** a new Kibana conversation is started
- **AND** ESDiag does not read or write a local conversation-selection file

### Requirement: Interrupted Conversation Is Not Retried
If an SSE request ends before completion after a conversation ID has been received, the command SHALL exit non-zero with a structured failure containing the safe conversation ID, a Kibana link when constructible, and `retry_safe: false`. It MUST NOT automatically submit the prompt again.

#### Scenario: Stream interrupts after conversation creation
- **WHEN** Agent Builder emits a conversation ID and the connection ends before message completion
- **THEN** the command returns a structured interrupted-conversation failure
- **AND** identifies the Kibana conversation as the recovery location
- **AND** does not issue a duplicate request

### Requirement: Portable Skill Is Script-Free
After structured outcomes, first-run onboarding, and `esdiag agent ask` are available, the canonical ESDiag skill and every generated plugin package SHALL contain no `scripts/` directory. Skill instructions SHALL compose native commands, use exact diagnostic identifiers from structured outcomes when available, delegate existing-diagnostic discovery and reasoning to Agent Builder, and hand missing configuration to `references/onboarding.md`.

#### Scenario: Canonical skill has no executable helpers
- **WHEN** the Agent Skill is validated
- **THEN** `.agents/skills/esdiag/scripts/` does not exist
- **AND** `SKILL.md` invokes no helper shell script or external HTTP/parser command

#### Scenario: Generated skills remain script-free
- **WHEN** the plugin skill is regenerated
- **THEN** `plugin/skills/esdiag/scripts/` does not exist
- **AND** Claude Code, Codex, and OpenCode receive the same native-command workflow
