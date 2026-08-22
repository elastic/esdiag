# Tasks

## 1. Collect scope (core + CLI)
- [x] 1.1 Constrain the Phase-1 input `Receiver` resolution: build a `Collect` receiver only for Elasticsearch/Kibana/Logstash; resolve Agent/platform inputs to a `Load` receiver.
- [x] 1.2 Make `esdiag collect` refuse an Agent/platform target as out-of-scope by design, with a message directing the caller to `read`/`Load` (distinct from an unimplemented-product error).
- [x] 1.3 Ensure the refusal path is separable from the not-yet-implemented path (the *reason* is preserved, not just the fact).

## 2. Collection definition
- [x] 2.1 Confirm `assets/<product>/sources.yml` exists only for Elasticsearch/Kibana/Logstash; ensure resolution never attempts an Agent/platform source set (ADR-0005).

## 3. Load-entry for product-provided diagnostics
- [x] 3.1 Ensure Agent and platform diagnostics take the `Load → [Process] → …` shape (no `Collect` stage) end to end.
- [x] 3.2 Confirm a platform bundle is Loaded then fanned out one level; assert ECE yields no included diagnostics (no application data).

## 4. Gap-kind distinction
- [x] 4.1 Classify a recognized-but-unprocessable child skip (Kibana/Agent processing) as *not-yet-implemented*, distinct from the by-design Collect refusal. Coordinate the carrier with the ADR-0016 `Skipped`-subtype change; do not redefine the outcome type here.
  - Held for Kibana and was inverted for Agent: the Agent arm raised the shared
    `Unsupported product or diagnostic bundle` error, which classifies as
    `ByDesign`, so a Loaded Agent diagnostic reported "skipped (by design)" —
    that ESDiag will never process it — while PR293 is in flight to do exactly
    that. Agent now carries its own not-implemented message alongside Kibana's.
    This is the erosion the design's risk section named: the classification hangs
    on which error string a stage raises, so sharing one between the two kinds is
    all it takes.

## 5. Web UI
- [x] 5.1 Ensure the `Collect` panel's remote-collect option applies only to API-collectable products; product-provided bundles arrive via `Upload` (`Load`).

## 6. Verification
- [x] 6.1 Test that `Collect` against Elasticsearch/Kibana/Logstash proceeds and against Agent/platform is refused by design.
- [x] 6.2 Test that an Agent/platform job begins with `Load` and contains no `Collect` stage.
- [x] 6.3 Test that an ECE bundle yields no included diagnostics.
- [x] 6.4 Test that a by-design Collect refusal and a not-yet-implemented child skip are reported distinctly (never conflated).
  - Covered only the Kibana child, which is why the Agent misclassification
    survived. Now `agent_child_is_skipped_as_work_in_progress_not_a_scope_boundary`
    covers the Loaded-Agent path end to end, and `the_two_gap_kinds_stay_separable`
    pins the classification of every error that carries a gap kind.
- [x] 6.5 Confirm the delta spec scenarios in `specs/collection-execution/spec.md` and `specs/included-diagnostic-jobs/spec.md` are covered.
