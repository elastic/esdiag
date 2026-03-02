## Context

ESDiag currently uses an explicit `ElasticsearchApi` enum in `src/processor/api.rs` that hardcodes exactly which APIs the application is capable of processing, along with their weights and standard groupings for different diagnostic types (minimal, standard, support, light). 

Issue #259 requested that we expand this to collect ALL APIs defined in the `sources.yml` file, specifically populating the `support` diagnostic type with every endpoint available, and populating the `light` type with every endpoint tagged with `tags: light`.

Currently, we only have specialized Processors and Receivers for roughly ~18 APIs. Adding ~80 new specialized structs and models is unnecessary since the legacy support-diagnostics tool simply dumps the raw JSON or TXT responses for these missing endpoints directly to disk without parsing them.

## Goals / Non-Goals

**Goals:**
- Dynamically parse the `assets/elasticsearch/sources.yml` to generate the lists of APIs to include in the `support` and `light` diagnostic types, eliminating hardcoded string lists for these categories where possible.
- Update `ApiResolver::resolve_es()` to accept any dynamically resolved string from the YAML file.
- Introduce a new generic `RawApi` data source and processor logic that can handle downloading *any* endpoint to disk even if we don't have a strongly-typed model for it in Rust.
- Execute the generic fetching alongside the specific thread pools in `ElasticsearchDiagnostic::process()`.

**Non-Goals:**
- Creating strongly-typed models, processors, or datastream mapping rules for the newly supported endpoints. They will strictly be collected and exported as raw files.
- Changing how the `minimal` or `standard` diagnostic profiles are curated (they can remain opinionated subsets).

## Decisions

**1. Deprecating the `ElasticsearchApi` Enum**
- **Decision**: Remove the `ElasticsearchApi` enum entirely in favor of treating API endpoints as plain `String` identifiers resolved directly from the keys of `sources.yml`. 
- **Rationale**: Hardcoding 90+ enum variants for dynamically defined APIs defeats the purpose of the YAML file. Treating the keys as the source of truth simplifies `api.rs`.

**2. Generic Raw API Data Source**
- **Decision**: Create a generic `RawEndpoint` struct that implements the `DataSource` trait. Its `name` will be dynamically injected during instantiation based on the API it represents.
- **Rationale**: We can spawn a single Tokio task in `collector.rs` that iterates over all unresolved APIs and passes them through `self.receiver.get_raw::<RawEndpoint>()` without needing any Serde parsing.

**3. Dynamic Tag Parsing in `ApiResolver`**
- **Decision**: Update `get_sources()` in `data_source.rs` to expose tags for each endpoint. `ApiResolver` will dynamically query `get_sources()` and iterate the keys to build the `support` list (all keys) and the `light` list (all keys with `tags: light`).
- **Rationale**: Keeps `api.rs` free of hardcoded endpoint lists and directly ties the collection profiles to the YAML definitions, making future maintenance trivially easy.

## Risks / Trade-offs

- **[Risk] Type Safety Loss** → **Mitigation**: Removing the `ElasticsearchApi` enum means we lose compile-time checking of API names. However, since the source of truth is a runtime YAML file, compile-time checking was already slightly divergent. We will add a runtime validation step when parsing `--include` / `--exclude` flags against the available YAML keys.
- **[Risk] Thread Starvation** → **Mitigation**: Executing 80+ new HTTP queries could slow down collection. We will use `futures::stream::StreamExt::for_each_concurrent` (or similar `tokio::spawn` batching) to parallelize the raw API collection efficiently rather than executing them sequentially.