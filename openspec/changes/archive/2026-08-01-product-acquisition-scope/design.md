# Design

Full rationale, rejected alternatives, and consequences live in
**`docs/adr/0019-acquisition-mode-api-collect-only-es-kibana-logstash.md`** (with the
`Skipped` refinement in **ADR-0016**); this design covers only the implementation
approach. The stage vocabulary (`Collect` vs `Load`) and phase rules come from
**ADR-0002**.

## Context

Every diagnostic enters through Phase 1 (ADR-0002): exactly one of `Collect` (pull live
APIs for a *new* diagnostic) xor `Load` (read an *existing* directory/bundle). Nothing
today binds those two inputs to the products they legitimately apply to, so `Collect`
reads as a general acquisition verb and a refused acquisition cannot say *why*.

## Approach

- **Bind the input phase to product acquisition mode.** The `Receiver` that resolves a
  Phase-1 input (CONTEXT.md, "Receiving") is chosen by product:
  - Elasticsearch, Kibana, Logstash → a `Collect` receiver (remote client over live
    REST APIs) *or* a `Load` receiver (read an existing bundle) — both valid.
  - Elastic Agent, ECE, ECK, KubernetesPlatform → a `Load` receiver **only**. No
    `Collect` receiver is constructed for these; the product provides its own bundle.
- **Refuse Collect for product-provided products with by-design guidance.** A `Collect`
  request targeting Agent/platform SHALL be refused as *out-of-scope by design* (not an
  "unimplemented product" error), and the message SHALL direct the caller to `Load`
  (CLI `read`, UI `Upload`).
- **Keep the collection definition three-product.** Per ADR-0005 the registry
  (`assets/<product>/sources.yml`) only ever describes API sources for the three
  API-collectable products; there is no Agent/platform `sources.yml`.
- **Reporting stays deferred to ADR-0016.** This change *names* the two gap kinds; the
  `Skipped`-subtype carrier that lets a report say "by-design" vs "not-yet-implemented"
  is delivered by the ADR-0016 change. Here, the not-yet-implemented child skip
  (Agent/Kibana processing) is asserted as distinct from the by-design Collect-scope
  refusal, without redefining the outcome type.

## Invariants

- Phase 1 is exactly one of `Collect` xor `Load` (ADR-0002); a job over an Agent or
  platform diagnostic therefore begins with `Load`, and its shape is
  `Load → [Process] → …`, never `Collect`.
- `Collect` targets ∈ {Elasticsearch, Kibana, Logstash}. No other value is a valid
  `Collect` target.
- A platform bundle is Loaded then fanned out one level (ADR-0001/`included-diagnostic-jobs`);
  ECE carries no application data, so it yields no included diagnostics.
- The by-design gap (Agent/platform API collection) and the not-yet-implemented gap
  (Kibana processing; Agent processing, PR293) are distinct and MUST NOT be conflated,
  even though both surface as a skip today.

## Deferred: trigger-then-Load

ADR-0019 notes a possible future capability where ESDiag *initiates* generation of an
ECE/ECK/Agent bundle (the product still collects itself) and then `Load`s the result —
a **delegated acquisition** flow. It is deliberately **out of scope here**: it is not
API `Collect` (ESDiag never pulls the product's APIs) and it does not change the
current Collect/Load binding. Recorded so a later change can add it without reopening
this scope boundary.

## Risks

- **Perceived regression.** Narrowing `Collect` may look like removed capability to a
  caller who expected to "collect" an Agent/platform; mitigated by the explicit
  by-design refusal that points at `Load` rather than failing opaquely.
- **Gap conflation resurfacing.** If the by-design refusal reuses the same generic
  "unsupported" path as the not-yet-implemented skip, the distinction erodes; the two
  paths must stay separable (the reason, not just the fact, is preserved).
