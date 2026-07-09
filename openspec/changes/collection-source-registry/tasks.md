# Tasks

## 1. Registry schema
- [x] 1.1 Extend the per-source `sources.yml` schema to the field set `{ key, versions, extension, subdir, retry, source_weight, processing_weight, streamable, processable, required, dependencies, collect_dependencies, tags }`, with serde types and defaults.
- [x] 1.2 Make `role` explicit with `processable` (collect-only vs processable); add a startup validation that a processable entry resolves exactly one impl and a bare entry is a valid collect-only source.
- [x] 1.3 Add a startup check that every processable key equals its registry key and `DataSource::name()`, failing fast on drift.

## 2. Key alignment (prerequisite)
- [x] 2.1 Reconcile existing dispatch/registry key drift to one canonical key per processable source (e.g. `pending_tasks` → `cluster_pending_tasks`); update `DataSource::name()`, dispatch, and `es_base_apis` references accordingly, while accepting legacy names as aliases.
- [x] 2.2 Confirm `_cat`/`.txt` collect-only entries and their JSON siblings coexist as two roles of one concept (no namespace conflict).

## 3. Two-axis weight (ADR-0017)
- [x] 3.1 Replace `ApiWeight { Heavy, Light }` and the `api.rs` `weight()` match with `source_weight` / `processing_weight` read from the registry.
- [x] 3.2 Point `collector.rs` collect scheduling at `source_weight`; wire processing scheduling to `processing_weight`; keep the weight→concurrency mapping as tunable policy (ADR-0018), not a hardcoded constant.
- [x] 3.3 Map legacy `Heavy`/`Light` onto the graded `source_weight` scale during migration.

## 4. Derivation — finish the migration (ADR-0005)
- [x] 4.1 Remove the `es_base_apis` Minimal/Standard `vec!` lists; derive Elasticsearch `minimal`/`standard` from registry tags/membership like `support`/`light`.
- [x] 4.2 Replace the hand-written `should_process` dispatch chain with a registry-iterated table keyed on the registry key, resolving one registered `DataSource`/`DocumentExporter` per processable source.
- [x] 4.3 Remove the `ElasticsearchApi` enum (and Kibana/Logstash siblings); if any is retained for ergonomics, generate it from or validate it against the registry at startup.
- [x] 4.4 Move `required` and `dependencies` out of `ProcessingOptionDef` into the registry, and keep collect-stage prerequisites in separate `collect_dependencies`.
- [x] 4.5 Make `streamable` an explicit flag; gate the streaming dispatch (`process_streaming_datasource::<T>`) on it instead of the hardcoded `IndicesStats`/`NodesStats`/`Snapshots` choice.

## 5. Reconciliation (ADR-0006)
- [x] 5.1 Write the reconciliation utility binary that overlays REST API files into ESDiag's `sources.yml` as a field-level merge, preserving `source_weight`/`processing_weight`/`streamable`/`processable`/`required`/`dependencies`/`collect_dependencies`/`tags`; verify `diags.yml` exists but defer OS-command overlay until ESDiag has a command-source transport model.
- [x] 5.2 Normalize upstream Java/NPM semver ranges into native Rust `semver` form during the overlay; store ranges in native form.
- [x] 5.3 Record deliberate divergences so reconciliation does not revert them.
- [x] 5.4 Document the required cadence (every application release AND every support-diagnostics release) and assign an owner.

## 6. Runtime semver simplification
- [x] 6.1 Remove the custom version-compatibility parser; resolve versions with stock `semver::VersionReq` against the normalized ranges.

## 7. Verification
- [x] 7.1 Tests: ES `minimal`/`standard`/`support`/`light` all derive from tags; no hardcoded list remains.
- [x] 7.2 Tests: registry-derived dispatch routes each processable key to its single impl; collect-only entry with no impl is not a wiring gap; unaligned key fails at startup.
- [x] 7.3 Tests: `source_weight` drives collect concurrency only; `processing_weight` drives processing concurrency only; asymmetric-cost source scheduled independently per stage.
- [x] 7.4 Tests: `streamable` flag selects the streaming vs buffered path.
- [x] 7.5 Tests: reconciliation overlay preserves enrichments, adds new sources, normalizes semver; stock `semver::VersionReq` resolves the normalized ranges.
- [x] 7.6 Confirm the delta-spec scenarios in `specs/{version-dependent-sources,api-selection,source-reconciliation}/spec.md` are covered.
