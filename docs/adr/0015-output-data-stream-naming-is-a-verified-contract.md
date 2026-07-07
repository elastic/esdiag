---
type: Reference
title: "Output data-stream naming is a verified contract, not a single source of truth"
status: accepted
tags: [repository, adr]
---

# Output data-stream naming is a verified contract, not a single source of truth

Unlike the input-side collection registry (ADR-0005), the **output data-stream name is
not derived from a single source of truth.** It spans three layers — processor code
(which emits to a stream), index templates (which match the stream pattern and define
its mapping), and Kibana dashboards (which query the stream and its fields) — and the
dashboards are **authored in Kibana and cannot be cleanly derived from code**. So the
name is treated as a documented *contract*, with the machine-checkable half verified.

## Considered options

- **Output-side SSOT / derivation (mirror ADR-0005).** Rejected: dashboards are
  hand-authored Kibana saved objects, not expressible as code variables, so full
  derivation is impossible; forcing it would either cripple dashboard authoring or
  leave the hardest layer unverified anyway.
- **Documented contract + partial verification (chosen).** A single naming convention
  (`{class}-{subtype}-esdiag`, class ∈ metrics|settings|logs|health), a test that the
  ESDiag-owned layers agree, and dashboards authored against the convention.

## Consequences

- **Processor ↔ index-template consistency is enforced by test** (both are
  ESDiag-owned): every emitted stream name must have a matching index template and
  vice versa. This catches the most common drift automatically.
- **Dashboards remain manually maintained** against the naming/field convention; a
  rename is a coordinated change across code, templates, and dashboards, and the
  dashboard half is a review/authoring discipline, not a derivation.
- **Explains the asymmetry with ADR-0005:** the input side is fully ESDiag-owned code
  and config (derivable); the output side terminates in authored Kibana artifacts
  (not derivable) — so the two seams get different treatments by necessity, not
  inconsistency.
