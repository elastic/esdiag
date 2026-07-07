---
status: accepted
---

# Weight is two axes: source load and processing cost

A data source's `weight` is split from a single binary (`Heavy`/`Light`) into **two
orthogonal per-source weights**: *source weight* (the load imposed on the system the
data source is pulled from) and *processing weight* (the ESDiag CPU/time to transform
it). They govern different stages, so conflating them is the same flattening seen in
`Product` (ADR-0001) and `RuntimeMode` (ADR-0007).

## Context

Today `ApiWeight { Heavy, Light }` conflates both and is consumed only for *collect*
scheduling (`collector.rs`: Heavy sequential, Light `buffer_unordered(5)`).
Processing cost is not modeled at all — yet a source can be cheap to fetch but
expensive to transform (a huge response), or the reverse.

## Decision

- **Source weight** governs *collect* concurrency and protects the source cluster.
- **Processing weight** governs *processing* concurrency/scheduling inside ESDiag.
- Both live per-source in the collection definition (ADR-0005).
- Likely a **graded scale** (e.g. 1–5) rather than binary; the exact granularity is
  TBD and should be validated against real load.

## Consequences

- Sources with asymmetric cost (cheap fetch / heavy transform, or vice versa) are now
  expressible and scheduled correctly on each stage independently.
- The registry field set (ADR-0005) carries `source_weight` and `processing_weight`
  instead of one `weight`.
- The *mapping* from weight → concurrency stays deployment-tunable policy (ADR-0018),
  not the hardcoded `buffer_unordered(5)`.
- Legacy `Heavy`/`Light` maps onto the new source-weight scale during migration.
