## Why

A data source is defined in six string-keyed places the compiler never
cross-checks — the `ElasticsearchApi` enum and its match arms, the
`ProcessingOptionDef` dependency list, the `es_base_apis` Minimal/Standard lists,
the `sources.yml` registry, and the hand-written `should_process("key")` dispatch
chain — so adding one source is ~10 scattered edits and every mismatch fails
silently. The registry migration is half-done: `Support`/`Light` already derive
from `sources.yml`, but `Minimal`/`Standard` are still hardcoded and dispatch is
still hand-authored. This change makes the per-product collection definition the
single source of truth and finishes the migration. Rationale: **ADR-0005**,
**ADR-0006**, **ADR-0017**.

## What Changes

- Make `assets/<product>/sources.yml` (the collection definition) authoritative:
  the collect list, the diagnostic-type sets, and the process dispatch are all
  *derived* from it, not maintained in parallel.
- **BREAKING (internal):** `es_base_apis` Minimal/Standard `vec!["…"]` lists are
  removed; those types derive from registry tags/membership like `Support`/`Light`
  already do.
- **BREAKING (internal):** the hand-written `should_process` dispatch chain becomes
  a registry-iterated table keyed on the registry key; the `ElasticsearchApi` enum
  (and Kibana/Logstash siblings) stops being a second hand-maintained list — it is
  removed or generated from/validated against the registry.
- Model a data source as **transport-neutral** (REST API | system command | file)
  with an explicit **role**: *collect-only* (e.g. `_cat` text APIs — saved for human
  reading, no processor, not a wiring gap) vs *processable* (carries a
  `DataSource`/`DocumentExporter` impl).
- **PREREQUISITE:** for every processable source its process-selection/dispatch key
  MUST equal its registry key and `DataSource::name()`. Existing drift (e.g.
  `pending_tasks` vs `cluster_pending_tasks`) is reconciled to one key per source.
- Move ESDiag execution metadata out of code and into the registry, so the field set
  becomes roughly `{ key, versions, extension, subdir, retry, source_weight,
  processing_weight, streamable, required, dependencies, tags }`.
- **BREAKING (internal):** replace the binary `ApiWeight { Heavy, Light }` with two
  graded per-source axes — `source_weight` (load on the source cluster, governs
  collect concurrency) and `processing_weight` (ESDiag transform cost, governs
  processing concurrency) — per **ADR-0017**. Make `streamable` an explicit flag
  instead of being implicit in which dispatch fn is called.
- Treat `support-diagnostics` (`elastic-rest.yml` + `kibana-rest.yml` +
  `logstash-rest.yml` + `diags.yml`) as a *reconciliation input*, not a runtime
  authority: a field-level overlay merges upstream REST `versions`/paths into
  ESDiag's files while **preserving ESDiag enrichments** (weights, tags,
  streamable), and **normalizes upstream's Java/NPM semver dialect into native
  Rust `semver` at the boundary** — which lets the runtime drop its custom
  version-compatibility parser and use stock `semver::VersionReq`. The script
  verifies `diags.yml` but defers OS-command overlay until ESDiag has a
  command-source transport model. Reconcile on every application release **and**
  every support-diagnostics release.

## Capabilities

### New Capabilities

- `source-reconciliation`: ESDiag owns its collection definitions and reconciles
  them from `support-diagnostics` via a field-level overlay that preserves ESDiag
  enrichments and normalizes the upstream semver dialect at the boundary; defines the
  required recurring cadence (ADR-0006).

### Modified Capabilities

- `version-dependent-sources`: the registry carries per-source execution metadata
  (two-axis weight, `streamable`, `required`, `dependencies`, `tags`, role) as the
  single source of truth; adds the processable-key-alignment invariant; runtime
  resolves versions with stock `semver::VersionReq` because ranges are normalized
  during reconciliation.
- `api-selection`: Elasticsearch `minimal`/`standard` derive from registry
  tags/membership (finishing the half-done migration); the process dispatch and the
  `ElasticsearchApi` enum are derived from the registry rather than hand-maintained.

## Impact

- **Core processing:** `api.rs` (`ElasticsearchApi` enum + `weight()` match removed),
  the `should_process` dispatch chain (→ registry table), `es_base_apis`
  Minimal/Standard lists, `ProcessingOptionDef` (`required`/`dependencies` move to
  registry), `collector.rs` concurrency (reads `source_weight`), the streaming
  dispatch (`process_streaming_datasource::<T>` gated by an explicit `streamable`
  flag), and the runtime semver compatibility shim (removed).
- **Assets:** `assets/{elasticsearch,kibana,logstash}/sources.yml` gain the expanded
  field set and reconciled/aligned keys; a reconciliation script is added.
- **CLI:** `--type`/`--include`/`--exclude` resolve entirely against registry
  keys/tags; weight tuning becomes data (overridable via `--sources`, no recompile).
- **Web UI:** advanced processing options continue to resolve from the authoritative
  registry (no behavioral change beyond the registry now being the sole authority).
- **Series:** builds on `platform-application-split` (tags carry platform/application
  per ADR-0001); weight→concurrency mapping stays deployment-tunable policy
  (ADR-0018).
