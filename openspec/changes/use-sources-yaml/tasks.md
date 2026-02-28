## 1. Global Sources Configuration

- [x] 1.1 Parse the embedded `assets/elasticsearch/sources.yml` into a statically accessible `HashMap<String, Source>` at application startup.
- [x] 1.2 Create helper functions for the `HashMap` to find a `Source` by its configured name and return an error if it's missing.

## 2. Refactor `DataSource` Trait

- [x] 2.1 Update the `DataSource::source` method signature in `src/processor/diagnostic/data_source.rs` to take `&semver::Version` alongside `PathType`, and return `eyre::Result<String>`.
- [x] 2.2 Temporarily update all current implementations of `DataSource` to compile with the new signature (e.g., in `src/processor/elasticsearch/tasks/data.rs` and any others).

## 3. Implement Dynamic Path Resolution

- [x] 3.1 Implement a function on `Source` to compute the file path (`PathType::File`) using `subdir`, the data source `name`, and `extension` (defaulting to `.json`).
- [x] 3.2 Implement a function on `Source` to compute the URL (`PathType::Url`) by iterating over its `versions` map, using `semver::VersionReq::parse` to test rules against the provided `semver::Version`.
- [x] 3.3 Ensure the `Url` resolver correctly handles missing matches and returns an appropriate error.

## 4. Integrate Configuration with Implementations

- [x] 4.1 Update `Tasks::source` in `src/processor/elasticsearch/tasks/data.rs` to fetch its `Source` configuration and return the generated path or URL, removing hardcoded logic.
- [x] 4.2 Validate and verify that `Tasks` and other data sources construct correctly when collected for specific simulated target host versions.

## 5. Verification

- [x] 5.1 Add unit tests for `Source` evaluating different semver rules from `sources.yml`.
- [x] 5.2 Run `cargo clippy --workspace --all-targets` and ensure no new warnings.
- [x] 5.3 Run `cargo test --workspace` and verify that all tests pass.