# Design

Full rationale, rejected alternatives, and consequences live in
**`docs/adr/0005-collection-definition-is-the-single-source-of-truth.md`**,
**`docs/adr/0006-own-sources-yml-reconcile-with-support-diagnostics.md`**, and
**`docs/adr/0017-weight-is-two-axes-source-load-and-processing-cost.md`**; this
design covers only the implementation approach, invariants, and risks. It depends on
`platform-application-split` (ADR-0001): `tags` carry platform/application.

## Context

Today one source touches six string-keyed sites (`ElasticsearchApi` enum + four
match arms, `ProcessingOptionDef` deps, `es_base_apis` Minimal/Standard,
`sources.yml`, and the `should_process("key")` chain) with no cross-check. The
registry is already the mature one — per-product, version-gated (`get_url(version)`),
embedded, `--sources`-overridable — and `Support`/`Light` already derive from it
(`get_source_keys` / `get_source_keys_with_tag`). The runtime carries a custom
version-compatibility parser because upstream ranges use a Java/NPM semver dialect
that the Rust `semver` crate does not accept verbatim.

## Approach

### Registry as authority

- Extend the per-source schema to the ADR-0005 field set:
  `{ key, versions, extension, subdir, retry, source_weight, processing_weight,
  streamable, processable, required, dependencies, collect_dependencies, tags }`.
  `processable` makes the role explicit for validation, and `required` being
  present marks a user-facing processing option.
- Replace the `es_base_apis` Minimal/Standard `vec!["…"]` with derivation via
  tags, exactly as `Support`/`Light` already resolve — so all four ES types come
  from one mechanism.
- Collection saves every resolved REST source through the registry key and raw
  request path. Typed `DataSource` structs are no longer part of collect dispatch;
  they are processing contracts only.
- Turn `should_process` into a table: iterate the registry, and for each
  *processable* key dispatch to its typed processor via a plain per-product
  registration table (`key -> impl DataSource/DocumentExporter`). The
  `ElasticsearchApi` enum is removed; if any variant is retained for ergonomics it is
  generated from or validated against the registry at startup, never authored in
  parallel.
- `weight()` match and the implicit streaming choice are deleted: `collector.rs`
  reads `source_weight` for collect scheduling, and the streaming dispatch
  (`process_streaming_datasource::<T>`) is gated on the explicit `streamable` flag.
  The collect and processing concurrency thresholds are deployment-tunable policy
  (`ESDIAG_COLLECT_POOL`, `ESDIAG_COLLECT_SEQUENTIAL_THRESHOLD`,
  `ESDIAG_PROCESS_CONCURRENT_THRESHOLD`) with defaults that preserve legacy
  behavior.

### Key alignment (prerequisite)

- For every *processable* source, one key is canonical: registry key ==
  process-selection/dispatch key == `DataSource::name()`. Reconcile existing drift
  (e.g. `pending_tasks` → `cluster_pending_tasks`) so weight, `streamable`, and
  type-membership attach to one key. This is *not* a namespace merge: `_cat` text
  APIs stay collect-only, their JSON siblings stay processable; a same-stem pair is
  two roles of one concept, not a conflict. Legacy CLI and saved-job names remain
  accepted through alias canonicalization, but generated catalogs and new manifests
  use canonical keys.

### Reconciliation (ADR-0006)

- A reconciliation utility binary overlays the upstream REST API files into ESDiag's
  `sources.yml` as a **field-level merge**: it updates `versions`/paths and
  preserves ESDiag-only fields (`source_weight`, `processing_weight`,
  `streamable`, `processable`, `required`, `dependencies`, `collect_dependencies`,
  `tags`). The merge must know which fields are ESDiag's — a blind copy would wipe
  the hand-tuned weights. The tool verifies the upstream
  `diags.yml` OS-command catalog path, but command entries are not merged until
  ESDiag has a command-source transport model.
- The tool converts upstream ranges into native Rust `semver` form at the boundary,
  so stored `sources.yml` is already in ESDiag's dialect. The runtime then uses stock
  `semver::VersionReq` and the compatibility shim is removed — the impedance is
  absorbed once, at reconciliation, not on every parse.
- Deliberate divergences (sources ESDiag adds/removes/corrects) are recorded so
  reconciliation does not silently revert them.

## Invariants

- Every processable source has exactly one canonical key (registry key ==
  dispatch key == `DataSource::name()`).
- A registry entry without a typed impl is a valid *collect-only* source, never a
  wiring gap; a processable source without a registry entry is an error.
- Diagnostic-type membership is expressed only as tags/membership in the registry —
  no hardcoded type list survives.
- All upstream-defined sources carry the `support` tag by default so ESDiag's
  support bundles remain support-diagnostics compatible.
- `source_weight` governs only collect concurrency; `processing_weight` governs only
  processing concurrency; neither is consulted by the other stage.
- The runtime binds to no upstream file; it reads only ESDiag's embedded (or
  `--sources`-overridden) definitions, already in native semver form.

## Risks

- **Silent drift becomes a load-bearing invariant.** Making dispatch derive from the
  registry means an unaligned key now fails validation at startup rather than
  no-op'ing — intended, but the key-alignment reconciliation must be complete before
  the dispatch table replaces the chain. Mitigation: a startup check that every
  processable key resolves to exactly one registered impl.
- **Weight regressions.** Collapsing then re-expanding `Heavy`/`Light` into a graded
  scale can change collect concurrency; legacy `Heavy`/`Light` maps onto the
  `source_weight` scale during migration (ADR-0017) and the weight→concurrency
  mapping stays tunable policy (ADR-0018), so tuning is data, not a recompile.
- **Stale version-gating.** Reconciliation is a recurring discipline; without an owner
  and cadence, new endpoints are missed and changed queries go stale (ADR-0006). This
  is the primary risk the reconciliation posture accepts, mitigated by tying it to
  both release cadences.
- **Semver normalization errors.** A mistranslated range at the boundary silently
  mis-gates a source. Mitigation: normalization runs once, in the tool, and is
  covered by tests over known upstream dialect forms before the runtime shim is
  removed.
