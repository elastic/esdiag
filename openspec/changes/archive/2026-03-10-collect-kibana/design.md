## Context

ESDiag already has the building blocks needed to talk to Kibana: `Product::Kibana`, `KibanaClient`, and host/auth plumbing all exist. The missing piece is the collection workflow. Today the shared source loader only embeds `assets/elasticsearch/sources.yml`, the `Source` model assumes every version rule maps to a plain string URL, `ApiResolver` only has concrete logic for Elasticsearch and Logstash, and `Diagnostic::try_new()` still returns an unimplemented error for Kibana.

The supplied `assets/kibana/sources.yml` is richer than the Elasticsearch catalog. A version rule can resolve either to a bare URL or to a structured object containing `url` plus execution metadata like `spaceaware` and `paginate`. That means Kibana support is not only a new product workflow, but also a shared-source model expansion that must remain backward compatible for Elasticsearch.

## Goals / Non-Goals

**Goals:**
- Enable `esdiag collect` to run successfully against Kibana targets.
- Use `assets/kibana/sources.yml` as the source of truth for Kibana endpoint selection, file naming, pagination, and space awareness.
- Extend shared source loading so both Elasticsearch and Kibana catalogs can be resolved through the same API.
- Define Kibana diagnostic type behavior that makes default `collect` usable even before curated Kibana subsets exist.
- Keep the implementation self-contained and cross-platform, reusing the current async Rust collection stack.

**Non-Goals:**
- Building strongly typed Kibana parsers, exporters, or enrichment logic for each collected API.
- Designing curated Kibana subsets beyond the bootstrap `minimal` profile and the full-catalog baseline used by the other types.
- Changing Elasticsearch collection semantics except where required to preserve compatibility with the new shared source model.
- Implementing Kibana processing or visualization features beyond raw collection output.

## Decisions

### 1. Use a richer, backward-compatible source configuration model
- **Decision**: Replace the current `versions: BTreeMap<String, String>` assumption with a product-agnostic model that can deserialize either a bare URL string or a structured version entry containing `url`, `spaceaware`, and `paginate`.
- **Rationale**: Kibana's catalog cannot be represented by the current type, and duplicating source parsing logic per product would fragment a capability that is already shared.
- **Alternative considered**: Add a separate Kibana-only parser. Rejected because it would duplicate semver resolution, file path logic, and global source-cache behavior.

### 2. Keep source catalogs keyed by product in one shared cache
- **Decision**: Expand `get_sources()` and `init_sources()` to load embedded source catalogs into a single product-keyed cache, with Elasticsearch and Kibana registered side by side.
- **Rationale**: Product-aware source lookup is already implied by `DataSource::product()`, and a shared cache keeps source resolution logic centralized for resolvers and collectors.
- **Alternative considered**: Add independent globals per product. Rejected because it complicates initialization and makes cross-product validation logic harder to share.

### 3. Introduce a dedicated Kibana execution plan instead of over-generalizing Elasticsearch enums
- **Decision**: Add a Kibana-specific resolver and collection path that produces raw execution plans containing the API name, resolved URL, output path, space-awareness, pagination field, and any derived scope metadata needed at runtime.
- **Rationale**: Kibana collection is fully source-driven and does not need a large typed enum surface on day one. A plan struct preserves room for future typed handling without forcing Elasticsearch and Kibana into the same enum hierarchy.
- **Alternative considered**: Reuse `ElasticsearchApi::Raw` for Kibana. Rejected because Kibana needs additional execution metadata that is not naturally expressible in the existing Elasticsearch API model.

### 4. Make Kibana bootstrap APIs mandatory
- **Decision**: Treat `kibana_status` as the required version-discovery API and `kibana_spaces` as the required space-discovery API. These remain in the plan even if a user excludes them or only selects space-aware APIs indirectly.
- **Rationale**: Versioned source resolution and space-aware expansion both depend on bootstrap data. Making them explicit dependencies keeps planning deterministic and makes the manifest reflect what was actually required to execute the run.
- **Alternative considered**: Issue hidden internal requests outside the resolved API plan. Rejected because it obscures what was collected and complicates debugging and test expectations.

### 5. Default Kibana type behavior should favor usability over premature curation
- **Decision**: Resolve Kibana `support`, `standard`, and `light` to the full Kibana source catalog for now, and keep `minimal` limited to the bootstrap APIs needed to identify the host and enumerate spaces.
- **Rationale**: The CLI default remains `standard`, so making `standard` usable is required for Kibana collection to work naturally. We do not yet have enough evidence to define smaller curated Kibana profiles.
- **Alternative considered**: Error on non-`support` Kibana types. Rejected because the default `collect` path would still fail unless users discovered the extra flag.

### 6. Preserve raw outputs and avoid response-shape rewriting
- **Decision**: Collect Kibana endpoints as raw responses and write them using the source-defined output naming, while adding scope-specific path segments for per-space and per-page artifacts so repeated requests do not overwrite each other.
- **Rationale**: This matches the parity goal, avoids inventing product-specific schema mergers, and keeps the collector robust for APIs that change shape over time.
- **Alternative considered**: Merge paginated results into one synthesized payload per API. Rejected because it changes the original response contract and creates edge cases for text or mixed-shape endpoints.

## Risks / Trade-offs

- **[Risk] Shared source parsing regressions for Elasticsearch** → **Mitigation**: keep deserialization backward compatible, preserve current file-path/url behavior for simple string entries, and add tests that cover both Elasticsearch and Kibana source formats.
- **[Risk] Kibana support runs can fan out heavily across spaces and paginated endpoints** → **Mitigation**: reuse bounded concurrency patterns and keep the execution plan explicit so pagination and space expansion can be throttled predictably.
- **[Risk] Partial permissions across spaces produce noisy or incomplete output** → **Mitigation**: record failures per request, continue collecting other spaces/endpoints, and ensure scoped outputs and summaries make partial coverage visible.
- **[Risk] Defaulting Kibana `standard` and `light` to the full catalog may collect more than some users expect** → **Mitigation**: document the temporary mapping in CLI help/tests and preserve `--include` / `--exclude` controls for operators who want narrower runs.

## Migration Plan

- No persisted data migration is required because this change only adds a new collection path and expands embedded source metadata handling.
- Roll out by landing the shared source-model changes first, then the Kibana resolver/collector path, then tests that cover default Kibana collection and scoped endpoint behavior.
- If rollback is needed, Kibana-specific resolver/collector wiring can be removed while keeping the pre-existing Elasticsearch path intact.

## Open Questions

- Should `--sources` remain a single override path applied only to the active target product, or should a future change support per-product override paths when multiple product catalogs are embedded?
- Do any Kibana endpoints in `assets/kibana/sources.yml` require special request headers or bodies beyond the current `KibanaClient` defaults once collection is implemented at scale?
