---
type: Reference
title: "Evolve the indexed data model via field aliases"
status: accepted
tags: [repository, adr]
---

# Evolve the indexed data model via field aliases

ADR-0001's split lands in the indexed docs as `diagnostic.application` (replacing
`diagnostic.product`) and `diagnostic.platform` (replacing the unused
`diagnostic.orchestration`). The `product` → `application` rename is bridged with
**Elasticsearch field aliases** so old and new dashboards keep working across old and
new indices during the transition; the aliases are removed later. This is the third
compatibility strategy, distinct from owned-file rewrite (ADR-0009) and
received-artifact tolerance (ADR-0010).

## Context

Indexed data is *semi-owned*: ESDiag controls the templates going forward (installed
by `setup`), but **cannot rewrite historical indices** produced by older versions. So
neither rewrite-on-first-read nor pure read-tolerance fits — field aliases bridge the
rename without touching stored documents.

## Decisions

- **`diagnostic.application` replaces `diagnostic.product`.** Both names resolve to the
  same underlying field via aliases in both directions, so dashboards querying either
  name work on both old and new indices during the transition.
- **`diagnostic.platform` replaces `diagnostic.orchestration`.** Non-breaking:
  `orchestration` is unused today, and no alias is needed because there are no
  platform-level dashboards yet — nothing queries it. The rename is not just the
  indexed field: the `orchestration` term is retired everywhere, including the
  in-code identifier and its derivation point (`Processor::start`, `mod.rs:420`, which
  derives it from the product and propagates it to children) — all become `platform`,
  sourced from the split `Platform` of ADR-0001.
- **Aliases are transitional** and removed once dashboards are updated and old indices
  age out of retention.

## Consequences

- No reindex and no clean break — historical indices remain queryable by both old and
  new dashboards for the alias lifetime.
- The removable aliases are the migration's only debt; track their removal so they
  don't linger indefinitely.
- Confirms the compat trilogy: **owned files → rewrite** (0009), **received artifacts →
  tolerate** (0010), **indexed data → field aliases** (this ADR).
