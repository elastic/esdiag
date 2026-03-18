## ADDED Requirements

### Requirement: Agent Source Catalog Registration
The system SHALL register `agent` as a supported product key in the embedded source catalog and the source loading pipeline. The `agent` product MUST have a required baseline source key (the version/identity file) that is always validated on load.

#### Scenario: Agent product is accepted by the source loader
- **GIVEN** the embedded source catalog is initialised
- **WHEN** sources for `agent` are requested
- **THEN** the loader returns the agent source map without error

#### Scenario: Agent identity source is required
- **GIVEN** a custom `sources.yml` override for `agent` that omits the identity source key
- **WHEN** the source loader validates the agent product sources
- **THEN** the loader returns an error naming the missing required key

### Requirement: Agent Diagnostic Type Profiles
The system SHALL resolve Agent diagnostic type profiles (`minimal`, `standard`, `light`, `support`) from the embedded `assets/agent/sources.yml` using the same tag-based resolution applied to Elasticsearch. Until curated tag subsets are defined, `support`, `standard`, and `light` SHALL resolve to the full agent source catalog. `minimal` SHALL resolve only the required baseline identity source.

#### Scenario: Agent support type collects all sources
- **GIVEN** a user runs `esdiag collect --type support` against an Agent bundle
- **WHEN** the API resolver evaluates the requested sources
- **THEN** it resolves all top-level keys present in `assets/agent/sources.yml`

#### Scenario: Agent minimal type collects only identity source
- **GIVEN** a user runs `esdiag collect --type minimal` against an Agent bundle
- **WHEN** the API resolver evaluates the requested sources
- **THEN** it resolves only the required baseline identity source key for Agent

### Requirement: Agent Include/Exclude Validation
The system SHALL validate `--include` and `--exclude` arguments for Agent collections against the keys defined in `assets/agent/sources.yml`. Invalid identifiers MUST cause an immediate error before any processing begins.

#### Scenario: Valid agent source identifier is accepted
- **GIVEN** a source key `agent_inspect` is defined in `assets/agent/sources.yml`
- **WHEN** the user runs `esdiag collect --include agent_inspect` against an Agent bundle
- **THEN** the resolver accepts the identifier and includes it in the execution plan

#### Scenario: Invalid agent source identifier is rejected
- **GIVEN** `not_a_real_agent_source` is not defined in `assets/agent/sources.yml`
- **WHEN** the user runs `esdiag collect --include not_a_real_agent_source` against an Agent bundle
- **THEN** the system exits with a validation error before processing begins
