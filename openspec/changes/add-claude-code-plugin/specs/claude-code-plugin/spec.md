## ADDED Requirements

### Requirement: Plugin Package Installable Into Claude Code
The project SHALL provide a distributable Claude Code plugin package containing valid plugin metadata and the portable ESDiag skill. Installation MUST NOT require a source checkout or host-specific copy of the skill instructions.

#### Scenario: Install packaged plugin
- **WHEN** a user installs the ESDiag plugin package into Claude Code
- **THEN** Claude Code discovers the ESDiag skill
- **AND** the package does not depend on repository-relative source paths at runtime

### Requirement: Bundled Operations Skill Is Single-Sourced
The distributable skill SHALL be generated from `.agents/skills/esdiag/`, and repository checks SHALL fail when generated package contents drift from that canonical source.

#### Scenario: Canonical skill changes
- **WHEN** a maintainer updates the canonical ESDiag skill
- **THEN** the sync workflow regenerates the packaged skill
- **AND** drift validation detects a stale package before release

### Requirement: Host Packaging Is A Thin Adapter
Claude Code, Codex, and OpenCode packaging SHALL expose the same portable ESDiag skill without duplicating workflow, configuration, onboarding, or analysis instructions in host-specific adapters.

#### Scenario: Multiple hosts consume the skill
- **WHEN** the Claude Code plugin, Codex package, and OpenCode discovery surface are validated
- **THEN** each points to or contains the same generated skill content
- **AND** host-specific metadata contains no independent copy of the diagnostic workflow

### Requirement: Plugin Defines No Independent Application Configuration
The plugin SHALL use ESDiag's canonical application configuration, saved hosts, keystore credentials, output-deployment resolution, and command options. It MUST NOT define separate Elasticsearch or Kibana URLs, Kibana-only credentials, inference routing, freshness windows, saved-job defaults, or local conversation-state storage.

#### Scenario: Output deployment is already configured
- **GIVEN** ESDiag resolves a configured processed-diagnostic output deployment
- **WHEN** the skill requests cluster-side analysis
- **THEN** it uses the native ESDiag command that resolves the same deployment
- **AND** the host package requires no plugin-specific connection settings

#### Scenario: Configuration is missing
- **WHEN** an ESDiag command reports missing application configuration
- **THEN** the skill routes the user to the onboarding reference and `esdiag init`
- **AND** does not ask the user to paste credentials into the agent conversation

### Requirement: First-Run Onboarding Is Separate From Skill Use
The plugin SHALL treat secure local initialization as an ESDiag-owned workflow documented in `references/onboarding.md`. Routine diagnostic review instructions MUST NOT reconstruct keystore creation, output deployment pairing, saved-host creation, or first-job setup through prompt-driven command sequences.

#### Scenario: Newly installed user asks for configuration help
- **WHEN** the user indicates that ESDiag was just installed or has not been configured
- **THEN** the skill loads the onboarding reference
- **AND** directs the user to run the terminal-native initialization workflow

#### Scenario: Configured user requests a review
- **GIVEN** ESDiag already has a valid output deployment and saved workflow
- **WHEN** the user requests diagnostic collection or review
- **THEN** the skill uses normal native commands directly
- **AND** does not repeat onboarding guidance

### Requirement: Cluster Provisioning Remains Separate
Plugin guidance SHALL reference cluster provisioning but MUST NOT implement licenses, asset installation, Agent Builder agents, inference services, connectors, or model credentials as part of host installation or ordinary client onboarding.

#### Scenario: Shared deployment is already provisioned
- **GIVEN** a user is configured for a shared ESDiag output deployment
- **WHEN** the plugin is installed
- **THEN** no local container runtime or cluster setup is required merely to use the skill

#### Scenario: Deployment lacks a usable model
- **WHEN** the first real Agent Builder request reports no usable model
- **THEN** the skill attributes the failure to deployment provisioning
- **AND** does not attempt to configure billing, a connector, or inference credentials

### Requirement: Diagnostic Collection Is Intent-Driven
The skill SHALL collect from a live cluster only when the user explicitly requests a new diagnostic or explicitly approves collection after being told which saved workflow will run. A reference to an existing or just-created diagnostic MUST NOT trigger collection.

#### Scenario: Explicit collection request
- **WHEN** the user asks to collect or run a new diagnostic
- **THEN** the skill may execute the configured saved workflow without a redundant confirmation
- **AND** reports the produced diagnostic or archive from its structured result

#### Scenario: Existing diagnostic request
- **WHEN** the user supplies a diagnostic identifier or refers to the diagnostic just processed
- **THEN** the skill uses that diagnostic for Agent Builder analysis
- **AND** does not collect another diagnostic

#### Scenario: Ambiguous request would require live collection
- **WHEN** the user asks a general cluster-health question and no existing diagnostic has been selected
- **THEN** the skill delegates discovery of existing diagnostics to Agent Builder first
- **AND** asks before running a live collection if the user subsequently wants fresh data

### Requirement: Diagnostic Review Skill Orchestration
The skill SHALL compose native ESDiag commands to collect or process diagnostics when authorized, consume the exact `diagnostic.id` from structured outcomes when one is newly created, and submit the user's question through the native Agent Builder ask command. It SHALL relay cluster-side analysis without reproducing diagnostic metrics or thresholds locally.

#### Scenario: Saved processing job produces an analyzed review
- **GIVEN** a configured saved processing job
- **WHEN** the user requests a fresh cluster review
- **THEN** the skill runs the saved job
- **AND** passes the returned diagnostic identifier in its Agent Builder prompt
- **AND** presents the returned Kibana conversation link

#### Scenario: Agent Builder discovers an existing diagnostic
- **WHEN** the user requests analysis without a newly processed diagnostic identifier
- **THEN** the skill asks the configured Agent Builder agent to identify and analyze the appropriate existing diagnostic
- **AND** does not execute a separate plugin-owned Elasticsearch freshness query
