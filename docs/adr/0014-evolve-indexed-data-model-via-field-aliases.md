---
type: Reference
title: "Evolve the indexed data model via field aliases"
status: accepted
tags: [repository, adr]
---

# Evolve the indexed data model via field aliases

ADR-0001's split lands in the indexed docs as `diagnostic.application` (replacing
`diagnostic.product`) and `diagnostic.platform` (replacing
`diagnostic.orchestration`). These provenance-field renames are bridged with
**Elasticsearch field aliases** so old and new dashboards keep working across old and
new indices during the transition; the aliases are removed later. This is the third
compatibility strategy, distinct from owned-file rewrite (ADR-0009) and
received-artifact tolerance (ADR-0010).

## Context

Indexed data is *semi-owned*: ESDiag controls the templates going forward (installed
by `setup`), but **cannot rewrite historical indices** produced by older versions. So
neither rewrite-on-first-read nor pure read-tolerance fits — field aliases bridge the
rename without touching stored documents. Adding an alias to a historical index is a
mapping addition, not a rewrite: no document changes and no reindex, which is what
makes the mirrored half of the bridge available at all.

## Decisions

- **`diagnostic.application` replaces `diagnostic.product`.** Both names resolve to the
  same underlying field via aliases in both directions, so dashboards querying either
  name work on both old and new indices during the transition. "Both directions" is a
  property of the *index pattern* a dashboard queries, not of one index: an alias must
  point at a concrete field, so each index carries exactly one of the pair as an alias,
  in whichever direction its own stored name dictates. The templates give every new
  index the legacy name as an alias; `setup` installs the mirror image — the current
  name as an alias to the stored legacy one — on indices that predate the rename. Both
  halves are needed, because a pattern spanning both generations resolves a name only
  if every index it touches does.
- **`diagnostic.platform` replaces `diagnostic.orchestration`.** The old field name
  resolves to the new one through a transitional alias while historical dashboards
  and retained indices age out. The rename is not just the indexed field: the
  `orchestration` term is retired everywhere, including the in-code identifier and
  its derivation point (`Processor::start`, `mod.rs:420`, which derives it from the
  product and propagates it to children) — all become `platform`, sourced from the
  split `Platform` of ADR-0001.
- **Aliases are transitional** and removed once dashboards are updated and old indices
  age out of retention.

## Consequences

- No reindex and no clean break — historical indices remain queryable by both old and
  new dashboards for the alias lifetime.
- **`setup` acquires a bridging step over existing indices**, which is best-effort by
  design: an index that cannot take a mapping update (closed, frozen, or write-blocked)
  costs the legacy-name resolution for that one index and must not fail installing the
  assets.
- The removable aliases are the migration's only debt. Half the removal trigger is now
  verifiable in the repository: the dashboards are shipped Kibana saved objects, so a
  test reports which provenance names they still query and refuses to let an alias be
  dropped while one depends on it. The other half — historical indices aging out of
  retention — remains operational.
- Confirms the compat trilogy: **owned files → rewrite** (0009), **received artifacts →
  tolerate** (0010), **indexed data → field aliases** (this ADR).
