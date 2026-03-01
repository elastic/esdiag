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