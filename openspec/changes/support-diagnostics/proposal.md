## Why

Currently, ESDiag collects a core subset of the Elasticsearch APIs defined in `sources.yml`. However, the legacy `support-diagnostics` tool fetches dozens of additional endpoints—from `cat` APIs to commercial ML and transform stats—to build a comprehensive snapshot of a cluster's state. To achieve full feature parity with the legacy diagnostic tool, ESDiag needs to implement receivers for all remaining endpoints listed in `assets/elasticsearch/sources.yml`.

## What Changes

- Add a generic Raw API Receiver logic (or discrete lightweight receivers) to download and save every endpoint currently missing from ESDiag's repertoire.
- Update `src/processor/api.rs` (`ElasticsearchApi` and `ApiResolver`) to support these new endpoints.
- Update the **support** diagnostic type (the default) to collect *all* entries defined in `sources.yml`.
- Update the **light** diagnostic type to dynamically include all entries tagged with `tags: light` in `sources.yml` (and whatever minimal requirements are needed).

## Capabilities

### New Capabilities
- `support-diagnostics-parity`: Expands Elasticsearch data collection to cover all endpoints defined in `sources.yml`, matching the legacy tool's capabilities. Defines the inclusion logic for `support` and `light` diagnostic types based on the YAML keys and tags.

### Modified Capabilities
- `api-selection`: (Delta) Update `ApiResolver` and related configurations to map the new API definitions and handle the "light" tags directly from the loaded `sources.yml`.

## Impact

- **Core Processing Logic**: Significantly increases the number of HTTP requests made during `esdiag collect elasticsearch`.
- **API Resolvers**: Needs an automated or expanded way to define the `ElasticsearchApi` enums and dependency lists to avoid manually hardcoding 90+ enum variants. 
- **Storage/Archive Size**: The resulting diagnostic archive will be larger and contain many more raw `.json` and `.txt` files.