---
type: Reference
title: "Manifests are additive-only and read-compatible forever; never migrated"
status: accepted
tags: [repository, adr]
---

# Manifests are additive-only and read-compatible forever; never migrated

The diagnostic manifest is a read-only interchange artifact, so ESDiag never
migrates it. Instead it maintains **permanent backward read-compatibility**: it must
always read manifests produced by `support-diagnostics` and by any prior ESDiag
version. ESDiag evolves the manifest **additively only** — it adds ESDiag-specific
properties and never removes, renames, or repurposes existing fields.

## Context

Unlike `saved_jobs.yml` (which ESDiag owns and can rewrite-on-first-read, ADR-0009),
a manifest lives inside a bundle — an external artifact received read-only, possibly
years old, produced by two different tools. Rewrite-on-first-read is impossible, and
ESDiag is the sole consumer of the properties it adds.

## Consequences

- **Manifest deserialization is tolerant:** unknown fields are ignored, ESDiag-added
  fields are optional/defaulted, and missing values are inferred.
- **`Product` → `Platform`/`Application` (ADR-0001) is handled by inference on old
  manifests**, not migration — the `Platform: Unknown` escape hatch plus indicators
  (`syscalls` folder ⇒ `SelfManaged`, `runner: ece` ⇒ `ECE`).
- **Additive-only is a hard constraint on manifest evolution:** new information goes
  into new optional fields; existing fields are never changed in meaning or shape.
  There is no manifest version gate and no migration path — read tolerance carries
  all compatibility.
- Owned files vs received artifacts get opposite strategies: **own-and-rewrite**
  for `saved_jobs.yml`, **tolerate-and-infer** for manifests/bundles.
