## Context

The current ESDiag diagnostic collection mechanism relies on hardcoded API paths and output file paths inside implementations of the `DataSource` trait. This does not scale well across different Elasticsearch versions, which often change API endpoints over time. The legacy support diagnostics tool solved this by externalizing these rules into a `sources.yml` file, which maps a "query label" (e.g., `tasks`) to rules determining both the API URL (based on semver rules matching the target node version) and the relative file path.

To increase compatibility and simplify maintenance, we need to adapt the `DataSource` trait to consume these `sources.yml` rules dynamically.

## Goals / Non-Goals

**Goals:**
- Load and parse `assets/elasticsearch/sources.yml` at runtime.
- Modify the `DataSource` trait to generate file paths and URLs dynamically based on the parsed configuration and the target host's version.
- Ensure that the generated strings for `PathType::File` match the logic `<subdir>/<name><extension>`.
- Ensure that the generated strings for `PathType::Url` match the query string corresponding to the `semver` rule that encompasses the target host's version.
- Remove hardcoded string returns from existing `DataSource` implementations like `Tasks`.

**Non-Goals:**
- Removing the `DataSource` trait entirely in favor of generic structures, since we still want to strongly type the data structures associated with each endpoint.
- Supporting legacy features of `sources.yml` that aren't used by ESDiag (e.g., `retry` limits or `tags`, unless directly required for pathing).

## Decisions

1. **`DataSource` Trait Signature Change:**
   - The `source()` method will be updated to return a `Result<String>` instead of `Result<&'static str>`, as the values will be dynamically generated.
   - The method signature will be updated to take the target host's `semver::Version` alongside the `PathType`.

2. **Configuration Loading & Caching:**
   - Use `lazy_static` or `std::sync::OnceLock` to parse and cache `sources.yml` into a global `HashMap<String, Source>` at application startup to avoid re-parsing YAML on every call.
   - The `assets` directory contains the file; we can use `include_str!` or a similar mechanism if we want the binary to be self-contained, or read it from disk if we prefer hot-reloading. Since ESDiag is a standalone CLI, we will embed the file using `include_str!("assets/elasticsearch/sources.yml")`.

3. **Semver Evaluation:**
   - We will use the `semver` crate's `VersionReq::parse` to evaluate the rules. Since the YAML file uses space-separated rules (e.g., `>= 0.9.0 < 5.1.1`), we may need to replace spaces with commas if `semver::VersionReq` strictly requires commas, though standard `semver` often parses space-separated bounds correctly.

4. **Error Handling:**
   - If a target version does not match any rule in the `versions` map, the `source()` method will return a distinct `eyre::Result::Err` indicating that the API is not supported on this version, allowing the processor to gracefully skip it.

## Risks / Trade-offs

- **[Risk]** The standard `semver` crate might fail to parse the NPM-style version rules found in `sources.yml` directly.
  - **Mitigation**: We will implement a preprocessing step on the rule strings (e.g., inserting commas where appropriate) before passing them to `semver::VersionReq::parse` if necessary.
- **[Risk]** Memory allocation overhead from returning `String` instead of `&'static str`.
  - **Mitigation**: This occurs once per data source during the diagnostic collection phase, making the performance impact negligible.
- **[Risk]** Legacy definitions use `/*/_ilm/explain` pattern which includes path wildcards.
  - **Mitigation**: We will substitute any path wildcards with the appropriate variables or preserve the wildcard strings to match ESDiag's current behavior if that's what's expected.