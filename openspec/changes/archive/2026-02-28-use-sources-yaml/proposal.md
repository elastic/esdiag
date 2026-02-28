## Why

ESDiag's diagnostic `collect` feature currently hardcodes the API endpoints and file paths for its data sources (e.g., in `DataSource` trait implementations). The legacy support diagnostics tool achieves broad version compatibility by using a `sources.yml` file to map data sources to their respective version-specific API endpoints and output file paths. Adopting the `sources.yml` configurations will allow ESDiag to seamlessly collect diagnostics across numerous Elasticsearch versions without requiring hardcoded logic for each version change.

## What Changes

- Modify the `DataSource` trait to determine the `PathType::File` and `PathType::Url` dynamically rather than using hardcoded values.
- Integrate the `sources.yml` configuration parsing (which is partially prepared in `Source` struct but unused) into the data source resolution process.
- Map the data source names to the keys in `sources.yml`.
- For `PathType::File`, construct the file path using `key.subdir` + `key` + `key.extension` from the YAML configuration.
- For `PathType::Url`, select the correct API endpoint by evaluating the `versions` semver rules against the target host's version.
- Update existing `DataSource` implementations (e.g., `Tasks` in `src/processor/elasticsearch/tasks/data.rs`) to leverage the new dynamic configuration system instead of returning hardcoded strings.

## Capabilities

### New Capabilities
- `version-dependent-sources`: Support for resolving diagnostic API endpoints and file paths dynamically based on the target host's version and the legacy `sources.yml` configuration file.

### Modified Capabilities

- 

## Impact

- **Core Processing Logic**: Affects how all implementations of the `DataSource` trait define their file and URL paths.
- **Dependencies**: Requires robust semver parsing (e.g., using `semver` crate) to evaluate the version constraints defined in `sources.yml`.
- **Target Products**: Specifically impacts the Elasticsearch diagnostic collection process, enabling support across a wide range of versions.