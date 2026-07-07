# Design

Full rationale, rejected alternatives, and consequences live in
**`docs/adr/0001-split-product-into-platform-and-application.md`**; this design covers
only the implementation approach. This change is **foundational** — changes 2–9 of the
architecture-review series reference `Platform`/`Application`.

## Context

`data::Product` (a single enum: `Agent, ECE, ECK, ElasticCloudHosted, Elasticsearch,
Kibana, KubernetesPlatform, Logstash, Unknown`) conflates two orthogonal axes and is
threaded through ~90 call sites, the manifest, and the `Processor::start` orchestration
derivation (`src/processor/mod.rs:420`).

## Approach

- Introduce `Platform` (required, total, incl. `SelfManaged` + `Unknown`) and
  `Application` (optional, closed 4-member set). Keep `Product` temporarily as a
  legacy alias to stage the ~90-site migration; it takes no new variants.
- **Detection:** move the `mod.rs:420` product→string derivation to a typed
  `Platform` detector driven by indicators (manifest `runner`, presence of a
  `syscalls` folder, cloud markers), defaulting to `Unknown`. This is best-effort by
  contract — callers must tolerate `Unknown`.
- **Propagation:** where children are spawned (`spawn_sub_processors`), set the child's
  `Platform` from the parent (today this rides on the inherited identifiers) — make it
  explicit and typed.
- **Envelope:** emit `diagnostic.platform` and `diagnostic.application`; the retired
  `diagnostic.orchestration` string is removed. Historical-index read compatibility
  (field aliases) is **out of scope here** and handled by `indexed-and-manifest-compat`
  (ADR-0014).

## Invariants (enforced at construction)

- Exactly one `Platform`; no "no platform" state.
- `Application` ∈ {`Elasticsearch, Kibana, Logstash, Agent`} or none — never a platform value.
- An included diagnostic is application-layer; its platform equals the parent's.

## Risks

- Wide blast radius (~90 sites) — mitigated by the transitional `Product` alias and by
  landing this change first, before dependents.
- Detection is heuristic; `Unknown` must be a first-class, non-failing value everywhere
  it is consumed.
