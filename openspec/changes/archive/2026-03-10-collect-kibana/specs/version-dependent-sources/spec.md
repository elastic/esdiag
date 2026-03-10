## MODIFIED Requirements

### Requirement: Dynamic Source Endpoint Resolution
The system SHALL resolve API endpoint queries dynamically using the target product and its corresponding `assets/<product>/sources.yml` mapping file. The system MUST support semver rules whose values resolve either directly to a URL string or to a structured source definition that includes `url` plus optional collection metadata such as pagination or space awareness.

#### Scenario: Host version matches a string-based semver rule
- **GIVEN** a product source configuration with version rules for a data source
- **WHEN** the target host's version matches exactly one string-valued semver rule
- **THEN** the system returns the API query string corresponding to that semver rule

#### Scenario: Host version matches a structured semver rule
- **GIVEN** a Kibana `sources.yml` configuration whose matching version rule contains a structured object with `url`, `spaceaware`, and `paginate` fields
- **WHEN** the target host's version matches that rule
- **THEN** the system returns the configured URL
- **AND** the system preserves the associated collection metadata for later execution planning

#### Scenario: Host version matches no semver rule
- **GIVEN** a product source configuration with version rules for a data source
- **WHEN** the target host's version matches none of the defined semver rules
- **THEN** the system returns an error indicating the API is unsupported on this version

### Requirement: Dynamic File Path Construction
The system SHALL construct local output file paths dynamically using the target product's `sources.yml` mapping file.

#### Scenario: Constructing a file path with a subdirectory and extension
- **GIVEN** a data source name, and a product `sources.yml` entry specifying a `subdir` and `extension`
- **WHEN** resolving the `PathType::File` for the data source
- **THEN** the system returns a file path formatted as `subdir/name.extension`

#### Scenario: Constructing a file path with default values
- **GIVEN** a data source name, and a product `sources.yml` entry specifying no `subdir` or `extension`
- **WHEN** resolving the `PathType::File` for the data source
- **THEN** the system returns a file path formatted as `name.json`

### Requirement: Global Sources Configuration Loading
The system SHALL load embedded product-specific `sources.yml` files into memory once during initialization and make them globally accessible to `DataSource` trait implementations and API resolvers.

#### Scenario: Initializing the configuration cache for multiple products
- **GIVEN** the application embeds both `assets/elasticsearch/sources.yml` and `assets/kibana/sources.yml`
- **WHEN** the system starts or is requested to resolve a data source for either product for the first time
- **THEN** both source catalogs are parsed into a globally accessible product-keyed structure

#### Scenario: Requesting an unknown data source for a product
- **GIVEN** the cached product source configuration
- **WHEN** the system is requested to resolve a data source name that does not exist for the target product
- **THEN** the system returns an error indicating the data source configuration is missing
