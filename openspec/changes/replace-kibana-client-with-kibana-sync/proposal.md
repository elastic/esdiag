## Why

ESDiag has a thin in-repo Kibana HTTP client that duplicates behavior now owned by the published `kibana-sync` crate. Replacing the local client reduces maintenance burden and lets Kibana collection and saved-object workflows share the same authentication, request routing, version parsing, space scoping, and multipart upload behavior.

## What Changes

- Add `kibana-sync` as the canonical Kibana client dependency for ESDiag.
- Replace `src/client/kibana.rs` internals, or the module entirely if call sites can be migrated cleanly, with `kibana_sync::KibanaClient`.
- Adapt ESDiag's `Auth` and `KnownHost` values into `kibana_sync::Auth` and `kibana_sync::KibanaClientBuilder`.
- Configure the shared client with ESDiag's existing Kibana request concurrency limit instead of adopting the crate default implicitly.
- Preserve ESDiag's Kibana receiver behavior: status/version lookup, space discovery, raw response capture, response timing, response size, and diagnostic manifest generation.
- Preserve Kibana collection behavior for source-defined endpoints, space-aware expansion, pagination, retry metrics, output layout, and archive finalization.
- Establish the dependency foundation for a follow-up change that expands ESDiag's bundled Kibana assets to cover the higher-level Kibana resource types supported by `kibana-sync`.
- Keep existing CLI and Web UI behavior unchanged; this is a core processing refactor, not a new user-facing command surface.

## Capabilities

### New Capabilities

- None.

### Modified Capabilities

- `kibana-diagnostic-collection`: Kibana collection must use `kibana-sync` as its HTTP client while preserving existing collection semantics and diagnostic metadata.

## Impact

- **Target product:** Kibana.
- **Core processing logic:** Affects Kibana receiver and collection execution paths.
- **Dependencies:** Adds `kibana-sync` from crates.io and removes direct Kibana client duplication where possible.
- **Code areas:** `Cargo.toml`, `src/client/kibana.rs`, `src/client/mod.rs`, `src/receiver/kibana.rs`, `src/processor/kibana/collector.rs`, and Kibana-focused tests.
- **Follow-up scope:** Bundled Kibana asset expansion should adopt `kibana-sync`'s saved object, space, agent, tool, and workflow support in a separate OpenSpec change.
- **CLI/Web UI:** No intended behavioral changes; existing collection commands, saved hosts, workflow jobs, and connection tests should continue to behave the same.
