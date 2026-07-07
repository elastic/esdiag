# Design

Full rationale, rejected alternatives, and consequences live in
**`docs/adr/0010`** (manifests additive-only, never migrated), **`0013`** (indexed data
model is ECS-inspired, source-API-aligned), **`0014`** (evolve the indexed model via
field aliases), and **`0015`** (output data-stream naming is a verified contract). This
design covers only the implementation approach. It **depends on**
`platform-application-split` (ADR-0001), which performs the source-side field/identifier
rename; this change adds the external-compatibility layers around it.

## Context

Three seams cross ESDiag's boundary and each evolves under a different ownership model,
so each gets a different compatibility strategy — the **compat trilogy**:

| Seam | Ownership | Strategy | ADR |
| --- | --- | --- | --- |
| `saved_jobs.yml` | owned | rewrite-on-first-read | 0009 |
| manifest / bundle | received read-only | tolerate + infer | 0010 |
| indexed documents | *semi-owned* (templates forward, indices historical) | field aliases | 0014 |

This change implements the last two rows.

## Approach

### Manifest read-compatibility (ADR-0010)

- Manifest deserialization is **tolerant**: unknown fields are ignored (serde
  non-strict), ESDiag-added fields are `Option`/`#[serde(default)]`, and absent values
  are inferred at read time. No version field gates behavior; read tolerance carries all
  compatibility.
- Evolution is **additive-only**: new information goes into new optional fields; existing
  fields never change meaning or shape. This is a hard constraint on future edits to the
  manifest model, not a runtime behavior.
- The legacy single-axis `Product` on old/foreign manifests is resolved to the
  `Platform`/`Application` pair by **inference** — `Platform: Unknown` as the escape
  hatch, refined by indicators (`syscalls` folder ⇒ `SelfManaged`, manifest
  `runner: ece` ⇒ `ECE`). The stored manifest is never rewritten. (The detector itself
  is owned by `platform-application-split`; here it is the read-compat consumer.)

### Indexed-data field aliases (ADR-0013, ADR-0014)

- The envelope stays **ECS-inspired but source-API-aligned**: the provenance layer
  (`diagnostic.*`, `cluster.*`) sits on top of a source-shaped payload; new fields mirror
  the source API first and borrow ECS conventions only where they don't obscure it.
- `diagnostic.application` **replaces** `diagnostic.product`, bridged by ES field aliases
  in **both directions** in the `esdiag@*` templates, so dashboards querying either name
  work on both old and new indices during the transition.
- `diagnostic.platform` **replaces** the unused `diagnostic.orchestration` with **no
  alias** — nothing queries `orchestration` (no platform-level dashboards), so the rename
  is non-breaking on the index side.

### Output data-stream naming contract (ADR-0015)

- One convention: `{class}-{subtype}[.sub]-esdiag`, class ∈ `metrics | settings | logs |
  health`.
- The two ESDiag-owned layers — processor-emitted stream names and index templates — are
  reconciled **by test**: every emitted stream must have a matching template and vice
  versa. Dashboards are the third layer; they are authored against the convention and
  maintained by review discipline, not derived.

## Invariants

- **Manifest reads never fail on unknown or missing fields.** A manifest from any
  `support-diagnostics` or prior ESDiag version deserializes successfully.
- **Additive-only:** no manifest field is ever removed, renamed, or repurposed.
- **`product`/`application` co-resolution is bidirectional** for the alias lifetime;
  neither name is authoritative over the other at query time.
- **No stored document or manifest is ever rewritten** by this change — compatibility is
  achieved entirely by read tolerance and index-level aliases.
- **Every emitted output stream name has exactly one matching index template**, and vice
  versa.

## Risks

- **Silent dashboard breakage** if an alias is missing or one-directional — mitigated by
  installing aliases in both directions and by dashboards continuing to query the legacy
  name until migrated.
- **Tolerant deserialization can mask genuinely malformed manifests** — mitigated by
  inferring/defaulting only the ESDiag-added axes, not core interchange fields.
- **Naming drift in the unverified (dashboard) layer** — the test covers only the two
  owned layers by necessity (ADR-0015); dashboard alignment stays a review concern.
- **Tracked debt:** the transitional `product`/`application` aliases are the migration's
  only debt. Their removal (once dashboards are updated and old indices age out of
  retention) MUST be tracked so they do not linger indefinitely.
