---
type: Reference
title: "The indexed data model is ECS-inspired but source-API-aligned"
status: accepted
tags: [repository, adr]
---

# The indexed data model is ECS-inspired but source-API-aligned

ESDiag's indexed diagnostic schema (the `esdiag@*` component and index templates) is
**ECS-inspired but not ECS-compliant**. Field shapes stay deliberately aligned to the
*source API's* output so that a user who knows the raw Elasticsearch/Logstash API
recognizes the fields immediately, rather than being remapped into strict ECS names.

## Considered options

- **Strict ECS compliance.** Rejected: remaps fields away from what users see in the
  raw API output, hurting recognizability and the "I know this from the API" UX; also
  a large, ongoing conformance burden.
- **ECS-inspired, source-API-aligned (chosen).** Borrow ECS structure/conventions
  where they help, but keep field names and shapes close to the source API.

## Consequences

- Field naming favors **source-API fidelity** over ecosystem interop; some divergence
  from ECS and ECS-based tooling is accepted deliberately.
- New fields should follow the same rule: mirror the source API's shape first, lean on
  ECS conventions only where they don't obscure the source.
- The provenance envelope (`diagnostic.*`, `cluster.*`) is the ESDiag-specific part
  layered on top of the source-shaped payload.
