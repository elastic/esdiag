---
type: Reference
title: "The collection definition is the single source of truth for data sources"
status: accepted
tags: [repository, adr]
---

# The collection definition is the single source of truth for data sources

Each product's `sources.yml` (the collection definition) is the authoritative
registry of its data sources, and the collect list, the process dispatch, and the
diagnostic-type sets are all *derived* from it. Today an API/data source is defined
in six string-keyed places that the compiler never cross-checks — the
`ElasticsearchApi` enum and its four match arms, the `ProcessingOptionDef`
dependency list, the `es_base_apis` Minimal/Standard lists, the `sources.yml`
registry, and the hand-written `should_process("key")` dispatch chain — so adding
one source is ~10 scattered edits and every mismatch fails silently.

## Prerequisite

`sources.yml` is a single namespace, grouped by output format. Sources have a
**role**: `cat`/`.txt` APIs are *collect-only* (human-readable duplicates saved into
the bundle, never processed, so no `DataSource`/`DocumentExporter` impl), while the
JSON APIs are *processable* (collected and transformed). A registry entry with no
processor is therefore normal — a collect-only source — not a wiring gap. This is
why `Support` collects the full set (`get_source_keys` returns everything, `cat_*`
included) while `Standard`/`Minimal` select curated *processable* subsets.

The alignment actually needed is narrower than a namespace merge: for a
**processable** source, its process-selection/dispatch key must equal its registry
key (and `DataSource::name()`). There is existing drift — e.g. dispatch/`es_base_apis`
use `pending_tasks` while `sources.yml`/`DataSource::name()` use
`cluster_pending_tasks` — and each such mismatch must be reconciled so weight,
streamable, and membership attach to one key per processable source.

## Considered options

- **Keep the parallel registries**, kept in sync by convention. Rejected: the
  failure modes are silent — add the enum variant but forget the dispatch arm and
  the source is collected yet never processed; omit the `es_base_apis` entry and it
  is defined yet never collected in Standard; typo any key and it is a no-op.
- **Derive everything from the collection definition (chosen).** The registry is
  already mature — per-product, version-gated (`get_url(version)`), embedded, and
  overridable via `--sources` — so it is the natural authority.

## Consequences

- **The half-done migration is completed.** `Support`/`Light` already derive their
  source set from the registry (`get_source_keys` / `get_source_keys_with_tag`);
  `Minimal`/`Standard` stop being hardcoded `vec!["…"]` lists and derive too (via
  tags/membership in the definition).
- **The `should_process` dispatch chain becomes a table**, iterated over the
  registry, rather than ~20 hand-written `if should_process(k) { process::<T>() }`
  blocks.
- **`ElasticsearchApi` (and its Kibana/Logstash siblings) stop being a second
  hand-maintained list.** The enum, if retained at all, is generated from or
  validated against the registry — not authored in parallel.
- **Adding a data source becomes one cohesive registration** (its definition entry
  plus its typed `DataSource`/`DocumentExporter` impl in a plain per-product table),
  not ten edits across `api.rs` and the dispatch chain.
- **The registry carries execution metadata, not just collection paths.** Every
  ESDiag-specific per-source value that lives in code today moves into the
  definition, so the full field set becomes roughly:
  `{ key, versions, extension, subdir, retry, source_weight, processing_weight,
  streamable, required, dependencies, tags }` — where `tags` covers diagnostic-type
  membership and (per ADR-0001) platform/application. Specifically these move out of
  code:
  - **weight** — from the `api.rs` `weight()` match; today a single `Light`/`Heavy`
    driving collect concurrency (`collector.rs`: Heavy sequential, Light concurrent).
    Per ADR-0017 it becomes two graded per-source axes (`source_weight`,
    `processing_weight`), so tuning becomes data (overridable via `--sources`, no
    recompile).
  - **streamable** — today *implicit* in which dispatch fn is called
    (`process_streaming_datasource::<T>` for only `IndicesStats`/`NodesStats`/
    `Snapshots`); becomes an explicit flag.
  - **required** and **dependencies** — from `ProcessingOptionDef`.
  - **diagnostic-type membership** ("bundle type") — from the hardcoded
    `es_base_apis` Minimal/Standard lists; expressed as tags like Light/Support
    already are.

  These are ESDiag concerns absent upstream (see ADR-0006). The **output target**
  (destination data stream/index) is deliberately *not* moved: one source fans out
  to many streams chosen inside the transform, so it stays in code with the
  transform.
- **A data source is not necessarily an API** (it may be a system command); the
  registry, not an `*Api` enum, is what models the full set — see the `Data source`
  and `Collection definition` glossary entries.
- Where the registry's *content* comes from is a separate decision (ADR-0006);
  this ADR only makes the registry authoritative within ESDiag.
