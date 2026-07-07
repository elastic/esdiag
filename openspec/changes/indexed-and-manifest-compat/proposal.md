## Why

ESDiag consumes and produces data across three seams that evolve at different rates,
and each demands a *different* compatibility strategy — the "compat trilogy": **owned
files → rewrite** (`saved_jobs.yml`, ADR-0009), **received artifacts → tolerate**
(manifests, ADR-0010), and **indexed data → field aliases** (ADR-0014). The
`platform-application-split` change renames the provenance fields at their source
(`diagnostic.product` → `diagnostic.application`, `diagnostic.orchestration` →
`diagnostic.platform`); this change lands the two *external-compatibility* halves that
split deliberately left out:

- **Manifests** are read-only interchange artifacts produced by `support-diagnostics`
  and by every prior ESDiag version. They can never be rewritten in place, so ESDiag
  must read them forever. Rationale: **ADR-0010**.
- **Indexed documents** already live in historical indices that ESDiag cannot reindex.
  The provenance-field rename would silently break existing Kibana dashboards unless the
  old and new names co-resolve. Rationale: **ADR-0013**, **ADR-0014**.
- The **output data-stream name** is a contract spanning processor code, index
  templates, and hand-authored dashboards — not derivable from a single source, so its
  ESDiag-owned half must be verified rather than trusted. Rationale: **ADR-0015**.

## What Changes

- **Manifest read-compatibility is permanent and additive-only.** ESDiag SHALL always
  read `support-diagnostics`- and older-ESDiag-produced manifests. It only *adds* its own
  optional properties and never removes, renames, or repurposes existing fields.
  Deserialization is tolerant: unknown fields are ignored, ESDiag-added fields are
  optional/defaulted, missing values are inferred. There is no manifest version gate and
  no migration path.
- **Legacy `Product` on old manifests is resolved by inference, not migration** — the
  `Platform: Unknown` escape hatch plus indicators (`syscalls` folder ⇒ `SelfManaged`,
  `runner: ece` ⇒ `ECE`) rather than any rewrite of the stored manifest.
- **The provenance-field rename is bridged by Elasticsearch field aliases.**
  `diagnostic.application` and legacy `diagnostic.product` resolve to the same underlying
  field via aliases in *both directions*, so old and new dashboards work across old and
  new indices. `diagnostic.platform` replaces the unused `diagnostic.orchestration`
  with **no alias** (nothing queries it — no platform-level dashboards). Aliases are
  transitional and removable once dashboards are updated and old indices age out.
- **The output data-stream naming contract is verified where ESDiag owns both ends.**
  A single convention `{class}-{subtype}-esdiag` (class ∈ `metrics | settings | logs |
  health`) is enforced by a test asserting processor-emitted stream names and index
  templates agree; dashboards remain authored against the convention by discipline.
- **The indexed envelope schema stays ECS-inspired but source-API-aligned** — new
  provenance/envelope fields mirror the source API's shape, not strict ECS.
- **BREAKING?** No. All changes are backward-compatible by construction: manifest reads
  are strictly widened, the `product`/`application` alias preserves existing dashboard
  queries, and `platform` replaces an unused field.

## Capabilities

### New Capabilities

- `manifest-compatibility`: permanent backward read-compatibility for manifests;
  additive-only evolution; tolerant deserialization; legacy `Product` resolved by
  inference rather than migration.
- `indexed-data-model`: the output data-stream naming contract (verified across
  ESDiag-owned layers); the ECS-inspired, source-API-aligned provenance envelope; and
  field-alias evolution of the renamed provenance fields.

### Modified Capabilities

- _(none — the in-code rename and its derivation point are owned by
  `platform-application-split` / `orchestration-metadata`; this change adds only the
  external-compatibility layers)_

## Impact

- **Manifest deserialization:** the manifest/`DiagnosticManifest` model — ESDiag fields
  become optional/defaulted; unknown fields ignored; no removals.
- **Indexed docs & templates:** the `esdiag@*` component/index templates installed by
  `setup` gain `diagnostic.application` ⇄ `diagnostic.product` aliases and the renamed
  `diagnostic.platform`.
- **Kibana dashboards:** continue querying either provenance name during the alias
  lifetime; authored against the `{class}-{subtype}-esdiag` convention.
- **Verification:** a new processor ↔ index-template consistency test.
- **Depends on:** `platform-application-split` (ADR-0001), which performs the source-side
  rename this change bridges. **Tracked debt:** the transitional aliases must be removed
  once dashboards are migrated and old indices expire.
