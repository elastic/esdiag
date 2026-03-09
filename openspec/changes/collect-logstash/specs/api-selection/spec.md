## ADDED Requirements

### Requirement: Logstash Support Diagnostic Type Expansion
The system SHALL resolve the Logstash `support` diagnostic type from the top-level keys defined in `assets/logstash/sources.yml` instead of a hardcoded API subset.

#### Scenario: Support type includes every configured Logstash source
- **GIVEN** `assets/logstash/sources.yml` defines the canonical Logstash sources
- **WHEN** the user runs `esdiag collect logstash --type support`
- **THEN** the resolver includes every top-level Logstash source key in the requested collection set before dependency resolution and exclusions are applied

### Requirement: Logstash Lighter Profile Stability
The system SHALL preserve bounded Logstash `minimal`, `standard`, and `light` profiles until `assets/logstash/sources.yml` provides metadata that defines lighter-weight subsets.

#### Scenario: Minimal Logstash collection stays narrow
- **GIVEN** the user runs `esdiag collect logstash --type minimal`
- **WHEN** the resolver builds the Logstash API plan
- **THEN** it includes only the required baseline Logstash node source and any required dependencies

#### Scenario: Standard and light keep the current bounded subset
- **GIVEN** the user runs `esdiag collect logstash --type standard` or `esdiag collect logstash --type light`
- **WHEN** the resolver builds the Logstash API plan
- **THEN** it includes the existing bounded Logstash subset rather than expanding to every key in `assets/logstash/sources.yml`

### Requirement: Logstash Identifier Normalization
The system SHALL accept both canonical Logstash `sources.yml` keys and legacy short Logstash identifiers for include/exclude handling, and it SHALL normalize the final execution plan to canonical source keys.

#### Scenario: User includes a legacy short Logstash identifier
- **GIVEN** the existing short identifier `node_stats` maps to the canonical source key `logstash_node_stats`
- **WHEN** the user runs `esdiag collect logstash --include node_stats`
- **THEN** the resolver accepts the request
- **AND** the final execution plan records `logstash_node_stats` as the collected API identifier

#### Scenario: User includes a canonical Logstash source key
- **GIVEN** `logstash_nodes_hot_threads_human` is defined in `assets/logstash/sources.yml`
- **WHEN** the user runs `esdiag collect logstash --include logstash_nodes_hot_threads_human`
- **THEN** the resolver accepts the request and includes that source in the final execution plan
