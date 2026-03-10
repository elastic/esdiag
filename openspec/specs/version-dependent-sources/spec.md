## ADDED Requirements

### Requirement: Dynamic Source Endpoint Resolution
The system SHALL resolve API endpoint queries dynamically using a target cluster version and the `assets/elasticsearch/sources.yml` mapping file.

#### Scenario: Host version matches a single semver rule
- **GIVEN** a `sources.yml` configuration with version rules for a data source
- **WHEN** the target host's version matches exactly one of the defined semver rules
- **THEN** the system returns the API query string corresponding to that semver rule

#### Scenario: Host version matches no semver rule
- **GIVEN** a `sources.yml` configuration with version rules for a data source
- **WHEN** the target host's version matches none of the defined semver rules
- **THEN** the system returns an error indicating the API is unsupported on this version

### Requirement: Dynamic File Path Construction
The system SHALL construct local output file paths dynamically using the `assets/elasticsearch/sources.yml` mapping file.

#### Scenario: Constructing a file path with a subdirectory and extension
- **GIVEN** a data source name, and a `sources.yml` entry specifying a `subdir` and `extension`
- **WHEN** resolving the `PathType::File` for the data source
- **THEN** the system returns a file path formatted as `subdir/name.extension`

#### Scenario: Constructing a file path with default values
- **GIVEN** a data source name, and a `sources.yml` entry specifying no `subdir` or `extension`
- **WHEN** resolving the `PathType::File` for the data source
- **THEN** the system returns a file path formatted as `name.json`

### Requirement: Global Sources Configuration Loading
The system SHALL load the `sources.yml` file into memory once during initialization and make it globally accessible to `DataSource` trait implementations.

#### Scenario: Initializing the configuration cache
- **GIVEN** the `assets/elasticsearch/sources.yml` file is embedded in the binary
- **WHEN** the system starts or is requested to resolve a data source for the first time
- **THEN** the YAML file is parsed and cached into a globally accessible structure

#### Scenario: Requesting an unknown data source
- **GIVEN** the cached `sources.yml` configuration
- **WHEN** the system is requested to resolve a data source name that does not exist in the YAML file
- **THEN** the system returns an error indicating the data source configuration is missing

### Requirement: Product-Scoped Sources Registry
The system SHALL load and expose source definitions separately for each supported product, including `assets/logstash/sources.yml` for Logstash.

#### Scenario: Logstash source definitions are available by product key
- **GIVEN** the application initializes its embedded source configuration
- **WHEN** a Logstash data source lookup is requested
- **THEN** the system resolves the lookup against the Logstash source registry rather than the Elasticsearch source registry

### Requirement: Receiver-Owned Source Product Resolution
The system SHALL choose the active `sources.yml` product from the current collect/process execution context rather than from a static property on each `DataSource`.

#### Scenario: Processing a Logstash bundle initializes Logstash file resolution once
- **GIVEN** a diagnostic bundle contains a manifest whose product is `logstash`
- **WHEN** `esdiag process` initializes the receiver for that bundle
- **THEN** the receiver selects the Logstash source registry for subsequent file-path and URL resolution
- **AND** Logstash `DataSource` implementations do not need to declare the product statically

### Requirement: SourceContext-Backed API Resolution
The system SHALL resolve `sources.yml`-backed API request paths, file paths, and output extensions through `DataSource` methods that consume a receiver-provided source context.

#### Scenario: API source path resolution uses receiver metadata
- **GIVEN** a receiver has identified the active sources product and target version for the current execution
- **WHEN** an API-backed `DataSource` resolves its request path or file path
- **THEN** it uses the receiver-provided source context rather than ad hoc product or version lookup at the call site

### Requirement: Bundle Metadata Files Are Not API Data Sources
The system SHALL treat bundle metadata files like `manifest.json` and `diagnostic_manifest.json` as explicit bundle-file reads rather than as `sources.yml`-defined `DataSource` entries.

#### Scenario: Processing reads bundle manifests without source lookup
- **GIVEN** a directory or archive receiver is identifying a diagnostic bundle
- **WHEN** it loads `diagnostic_manifest.json` or falls back to `manifest.json`
- **THEN** it reads those files directly by filename
- **AND** no `sources.yml` lookup is required for bundle manifest loading

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