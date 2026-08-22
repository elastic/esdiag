## ADDED Requirements

### Requirement: Analysis Delegated To The Cluster Agent
Diagnostic reasoning SHALL be performed by the configured Kibana Agent Builder agent in the output deployment. The local skill MUST NOT reimplement the agent's ES|QL, metrics, thresholds, or recommendations.

#### Scenario: Diagnostic question is submitted
- **WHEN** the skill submits a diagnostic question through ESDiag's native Agent Builder command
- **THEN** the configured cluster-side agent performs the analysis
- **AND** the local agent relays the completed markdown without re-deriving it

### Requirement: Real Analysis Is Persisted In Kibana History
Every real analysis and follow-up SHALL use the Agent Builder conversation API so the resulting thread remains visible and resumable in Kibana. The client SHALL return the conversation identifier and a Kibana handoff link and MUST NOT require a second local conversation-history store.

#### Scenario: First question creates a conversation
- **WHEN** a new analysis completes
- **THEN** the response identifies the persisted Kibana conversation
- **AND** the user can follow its Kibana link to continue the thread

#### Scenario: Explicit follow-up continues a conversation
- **GIVEN** a prior response returned a conversation identifier
- **WHEN** the caller supplies that identifier with a follow-up question
- **THEN** Agent Builder appends the turn to the same Kibana conversation

### Requirement: Streaming Progress Is Presented Safely
The ESDiag Agent Builder client SHALL consume Kibana's asynchronous SSE response internally and present reasoning and tool progress as operational status while reserving the structured terminal outcome for the completed response.

#### Scenario: Long analysis emits progress
- **WHEN** Agent Builder emits reasoning and tool events before completion
- **THEN** the user sees incremental progress
- **AND** progress does not contaminate the structured completed response

### Requirement: Exact New Diagnostic Identifier Is Supplied
When ESDiag has just processed a diagnostic, the skill SHALL include the exact `diagnostic.id` from the native structured outcome in the Agent Builder prompt. It MUST NOT infer the identifier from timestamps, prose, or a separate latest-diagnostic query.

#### Scenario: Processing returns an identifier
- **WHEN** a process or saved-job outcome contains a diagnostic identifier
- **THEN** the subsequent Agent Builder prompt includes that identifier
- **AND** no discovery query is needed to identify the newly created diagnostic

### Requirement: Existing Diagnostic Discovery Stays In Agent Builder
When no newly created or explicitly supplied diagnostic identifier exists, the skill SHALL allow the configured Agent Builder agent to discover appropriate existing diagnostic data with its installed tools. ESDiag and the plugin MUST NOT maintain a second freshness query or diagnostic-selection policy.

#### Scenario: General review of existing data
- **WHEN** the user asks for a review without requesting fresh collection and without supplying an identifier
- **THEN** the Agent Builder agent performs diagnostic discovery and analysis
- **AND** the local client does not query Elasticsearch directly for freshness

### Requirement: Unstructured Response Handling
The completed Agent Builder message SHALL be treated as unstructured markdown for presentation. The local skill MUST NOT parse findings into automated remediation, collection, retry, or severity decisions.

#### Scenario: Critical finding is returned
- **WHEN** Agent Builder describes a critical finding
- **THEN** the skill presents that finding to the user
- **AND** does not take remediation or issue another paid request based on parsing the prose

### Requirement: Analysis Failure Attribution
The client SHALL distinguish missing local/output configuration, Agent Builder authorization, unavailable configured agent, missing deployment model, unknown diagnostic, and interrupted-conversation failures without exposing credentials.

#### Scenario: Local output configuration is missing
- **WHEN** the Agent Builder command cannot resolve the output deployment's Kibana viewer
- **THEN** the failure directs the user to first-run onboarding
- **AND** does not describe the problem as cluster provisioning

#### Scenario: Deployment has no usable model
- **WHEN** Agent Builder rejects the first real question because no model is available
- **THEN** the failure identifies a deployment prerequisite
- **AND** does not retry or attempt connector configuration

#### Scenario: Stream interrupts after conversation creation
- **WHEN** a conversation identifier is received but the response ends before completion
- **THEN** the failure identifies the existing Kibana conversation as the recovery location
- **AND** marks automatic retry unsafe
