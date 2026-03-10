## Context

`esdiag collect` currently has a product-aware processing pipeline for Logstash, but its collection pipeline is still Elasticsearch-only. The shared source registry embeds `assets/elasticsearch/sources.yml` and the Logstash API resolver still hardcodes only `node` and `node_stats`, even though `assets/logstash/sources.yml` defines the full support surface (`logstash_health_report`, `logstash_node`, `logstash_nodes_hot_threads`, `logstash_nodes_hot_threads_human`, `logstash_node_stats`, `logstash_plugins`, `logstash_version`).

There is also a naming mismatch between existing typed Logstash data sources and the YAML file: the code uses short identifiers such as `node`, `node_stats`, `plugins`, and `version`, while the YAML uses canonical `logstash_*` keys. The design needs to add full Logstash collection support without breaking existing processor-side parsing or forcing a broad rename across the codebase.

## Goals / Non-Goals

**Goals:**
- Add a Logstash collection path that can execute `minimal`, `standard`, `support`, and `light` diagnostic types.
- Load Logstash source definitions from `assets/logstash/sources.yml` and use them for URL/file resolution, validation, and support-profile expansion.
- Support canonical Logstash source keys while preserving compatibility with existing short identifiers used by typed data sources.
- Collect typed Logstash APIs through dedicated handlers where they already exist, and use generic raw collection for the remaining Logstash endpoints.
- Emit a diagnostic manifest for Logstash collections that records the resolved API list.

**Non-Goals:**
- Changing the Logstash processing/export behavior for already supported files after collection completes.
- Reworking Elasticsearch collection into a fully generic multi-product collector in this change.
- Defining new `tags` semantics for Logstash `light` collection before the source file actually carries those tags.

## Decisions

### 1. Add a dedicated `LogstashCollector`

- **Decision**: Introduce a Logstash-specific collector and dispatch to it from the top-level collector factory instead of trying to fully genericize the existing Elasticsearch collector first.
- **Rationale**: The current collector already mixes product-specific manifest bootstrap, API enums, and typed/raw save dispatch. A parallel Logstash collector is the lowest-risk path to ship parity quickly while still reusing the same retry, archive, and raw-fetch patterns.
- **Alternative considered**: Refactor immediately to a single generic collector for all products.
- **Why not now**: That would enlarge the change substantially and couple Logstash parity to a broader collector architecture rewrite.

### 2. Make the source registry product-scoped and receiver-owned at runtime

- **Decision**: Extend the global source registry to embed and expose both Elasticsearch and Logstash `sources.yml` files under separate product keys, but resolve those product keys from the active receiver or collect command context rather than from `DataSource` types. Represent that runtime selection with a shared source context passed into `DataSource` path-resolution methods.
- **Rationale**: Each `collect` or `process` execution operates on exactly one product, so the receiver already has the right boundary for selecting the correct source registry. Keeping `DataSource` product-agnostic avoids leaking transport or manifest context into every data model type while still giving call sites explicit source-path APIs.
- **Alternative considered**: Keep product selection on `DataSource` implementations via a static method such as `product()`.
- **Why not now**: That makes source lookup an intrinsic property of the data type even though the real runtime boundary is the receiver/command execution context, and it becomes awkward for fixed bundle files like `manifest.json`.

### 3. Separate bundle metadata files from API data sources

- **Decision**: Stop treating bundle metadata files such as `manifest.json` and `diagnostic_manifest.json` as `DataSource` implementations, and read them explicitly through receiver bundle-file helpers.
- **Rationale**: Those files are archive/directory artifacts, not `sources.yml`-defined APIs. Removing them from `DataSource` keeps API path resolution limited to real source entries and removes special-case checks from archive/directory receivers.
- **Alternative considered**: Keep manifest files inside `DataSource` with special-case filename handling.
- **Why not now**: That mixes bundle metadata with API sources and weakens the boundary introduced by receiver-owned source resolution.

### 4. Normalize Logstash APIs to canonical source keys

- **Decision**: Resolve Logstash collections internally using the canonical `assets/logstash/sources.yml` keys (`logstash_node`, `logstash_node_stats`, etc.), while accepting existing short aliases (`node`, `node_stats`, `plugins`, `version`, `hot_threads`) for backward compatibility.
- **Rationale**: The YAML keys must remain the source of truth for support collection and manifest reporting, but alias support prevents breaking existing include/exclude usage and avoids renaming every Logstash data source immediately.
- **Alternative considered**: Rename all Logstash data sources and CLI-facing identifiers to canonical keys in one step.
- **Why not now**: That would create unnecessary processor churn and increase migration risk.

### 5. Split execution into typed and raw Logstash endpoints

- **Decision**: Keep explicit typed handlers for `logstash_node` and `logstash_node_stats`, and collect the remaining Logstash sources as raw files resolved from YAML.
- **Rationale**: These two endpoints already map directly to established Logstash processing logic, while `logstash_version`, `logstash_plugins`, and the hot-threads variants only need archive-compatible file output for parity. Raw collection avoids creating extra enums or save branches for every new source.
- **Alternative considered**: Add typed collection handlers for every Logstash source already represented by a `DataSource`.
- **Why not now**: It provides little value over raw collection, and it increases duplicate-fetch risk for sources that only need to be stored.

### 6. Use dedicated Logstash transport types

- **Decision**: Add dedicated `LogstashClient` and `LogstashReceiver` implementations for Logstash known hosts instead of routing Logstash traffic through the Elasticsearch client and receiver.
- **Rationale**: Logstash only needs a light reqwest-based HTTP wrapper like Kibana, and using dedicated transport types keeps product detection, root-response validation, and request semantics explicit at the client/receiver boundary.
- **Alternative considered**: Continue treating Logstash as compatible with the Elasticsearch client and receiver stack.
- **Why not now**: The Elasticsearch transport has product-specific assumptions that are not part of the Logstash contract, and sharing that path obscures bugs in future Logstash-specific behavior.

### 7. Preserve current lighter profiles until Logstash tags exist

- **Decision**: Keep `minimal` mapped to the node baseline, and keep `standard`/`light` mapped to the current bounded Logstash subset until `assets/logstash/sources.yml` grows tags or other metadata for lighter profiles.
- **Rationale**: The user request is specifically about full support collection from `sources.yml`. The file currently defines full coverage, not differentiated light-profile metadata.
- **Alternative considered**: Make `light` or `standard` dynamically include all Logstash sources immediately.
- **Why not now**: That would silently change the performance profile of non-support runs without any source metadata justifying the change.

### 8. Add ignored external-service compatibility coverage

- **Decision**: Add ignored integration tests that target externally managed Logstash `6.8.x`, `7.17.x`, `8.19.x`, and `9.x` instances.
- **Rationale**: The main behavioral risk in this change is version-sensitive endpoint resolution from `assets/logstash/sources.yml`. Ignored tests let the repository encode the supported compatibility matrix without making CI depend on always-available external services.
- **Alternative considered**: Limit verification to unit tests around semver resolution and local mocks.
- **Why not now**: Unit tests are necessary but do not prove that real Logstash versions expose the expected endpoints or response shapes across the full target matrix.

## Risks / Trade-offs

- **[Risk] Alias normalization creates duplicate planning paths** → **Mitigation**: Normalize requested Logstash identifiers to canonical source keys before deduplication, dependency resolution, and manifest generation.
- **[Risk] Product dispatch remains partially duplicated between Elasticsearch and Logstash** → **Mitigation**: Reuse the same raw-save, retry, and manifest patterns so a future generic refactor can collapse the two collectors cleanly.
- **[Risk] Logstash `light` remains only partially source-driven** → **Mitigation**: Scope this change to support parity now and document that lighter profiles can become tag-driven once the Logstash source file adds the required metadata.
- **[Risk] Some Logstash endpoints may not be available on older versions** → **Mitigation**: Continue using semver-based source resolution and skip unsupported endpoints the same way Elasticsearch raw collection already does.
- **[Risk] External compatibility tests may be unavailable in normal CI** → **Mitigation**: Mark them ignored, document the required service configuration, and keep core logic covered by unit tests.

## Migration Plan

- No user-facing migration is required for existing short Logstash include/exclude identifiers.
- New support collections will record canonical Logstash source keys in the manifest and save additional files defined by `assets/logstash/sources.yml`.

## Open Questions

- Whether `logstash_plugins` and `logstash_version` should remain raw-only during collection long-term, or later gain explicit typed save branches for symmetry with processing.
