## Why

ESDiag can target Kibana hosts, but `collect` does not yet implement a Kibana diagnostic workflow. We now have a curated `assets/kibana/sources.yml` inventory, and PR260 established the expectation of full support-oriented API collection driven by source definitions, so Kibana needs the same source-driven collection path to reach parity.

## What Changes

- Add Kibana collection support to `esdiag collect` so a Kibana target can execute a support-style diagnostic run instead of failing as unimplemented.
- Introduce Kibana raw API collection driven by `assets/kibana/sources.yml`, including version-aware endpoint selection, output naming, pagination, and space-aware endpoint expansion.
- Extend shared source loading and resolution so product-specific source catalogs can be loaded and queried for Kibana as well as Elasticsearch.
- Update API selection and validation to resolve Kibana API identifiers from the Kibana source catalog rather than Elasticsearch-only definitions.

## Capabilities

### New Capabilities
- `kibana-diagnostic-collection`: Collect Kibana support diagnostics from the Kibana source catalog, including raw endpoint fetching, space-aware expansion, pagination, and source-defined output paths.

### Modified Capabilities
- `version-dependent-sources`: Extend source loading and endpoint resolution to support multiple product catalogs, including `assets/kibana/sources.yml`.
- `api-selection`: Resolve diagnostic type contents and include/exclude validation against the target product's source catalog so Kibana collections use Kibana API keys.

## Impact

- **Core Processing Logic**: Adds a new Kibana collection path and product-aware source resolution in the shared diagnostic pipeline.
- **CLI Behavior**: `esdiag collect` for Kibana will move from an unimplemented error to an executable support collection workflow.
- **HTTP Collection**: Kibana collection will issue many additional requests, including per-space and paginated endpoints defined in `assets/kibana/sources.yml`.
- **Output Shape**: Kibana diagnostics will produce raw JSON and text artifacts using source-defined filenames and subdirectories.
