## ADDED Requirements

### Requirement: Product-Scoped Sources Registry
The system SHALL load and expose source definitions separately for each supported product, including `assets/logstash/sources.yml` for Logstash.

#### Scenario: Logstash source definitions are available by product key
- **GIVEN** the application initializes its embedded source configuration
- **WHEN** a Logstash data source lookup is requested
- **THEN** the system resolves the lookup against the Logstash source registry rather than the Elasticsearch source registry

### Requirement: Logstash Source URL Resolution
The system SHALL resolve Logstash API request paths dynamically from `assets/logstash/sources.yml` using the target Logstash version.

#### Scenario: Matching a Logstash version rule
- **GIVEN** the source key `logstash_health_report` is defined in `assets/logstash/sources.yml` with a semver rule for `>= 8.16.0`
- **WHEN** the target Logstash version is `8.16.0` or newer
- **THEN** the system resolves the request path `/_health_report` for that source

### Requirement: Alias-Backed Logstash File Resolution
The system SHALL allow Logstash data sources with legacy short internal names to resolve file paths through canonical `logstash_*` source keys.

#### Scenario: Short Logstash data source name resolves through canonical alias
- **GIVEN** a Logstash data source uses the internal name `node` and the canonical source key `logstash_node`
- **WHEN** the system resolves the file path for that data source
- **THEN** it returns the file path defined by the `logstash_node` source configuration
- **AND** the resulting output filename is `logstash_node.json`
