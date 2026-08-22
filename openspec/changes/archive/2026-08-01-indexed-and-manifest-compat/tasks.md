# Tasks

## 1. Manifest read-compatibility (ADR-0010)
- [x] 1.1 Make the manifest / `DiagnosticManifest` deserialization tolerant: ignore unknown fields, mark all ESDiag-added properties `Option` / `#[serde(default)]`.
- [x] 1.2 Confirm no existing interchange field is removed, renamed, or repurposed; document the additive-only constraint at the model.
- [x] 1.3 Resolve legacy/absent `Product` on read by inference (default `Platform::Unknown`, refined by `syscalls` folder ⇒ `SelfManaged`, `runner: ece` ⇒ `ECE`), without rewriting the manifest. (Detector owned by `platform-application-split`.)

## 2. Indexed-data field aliases (ADR-0013, ADR-0014)
- [x] 2.1 In the `esdiag@*` templates, add bidirectional field aliases so `diagnostic.product` and `diagnostic.application` resolve to the same underlying field.
  - Templates carry only one half, and can only ever carry one half:
    bidirectionality belongs to the *index pattern* a dashboard queries, while a
    field alias must point at a concrete field, so each index gets exactly one
    of the pair. New indices alias the legacy name to the current one; verification
    found the mirror missing, which left `diagnostic.application` matching nothing
    in every pre-rename index. `setup::install_provenance_aliases` now installs
    that mirror over existing indices — idempotent, and best-effort so an index
    that cannot take a mapping update does not fail asset installation.
- [x] 2.2 Replace `diagnostic.orchestration` with `diagnostic.platform` in the templates, keeping `diagnostic.orchestration` as a transitional alias.
- [x] 2.3 Keep new/renamed envelope fields ECS-inspired but source-API-aligned; layer `diagnostic.*` / `cluster.*` over the source-shaped payload.
- [x] 2.4 Record the transitional aliases as tracked debt with a removal trigger (dashboards migrated + old indices aged out).
  - Half the trigger is now verifiable rather than remembered. The dashboards ship
    as Kibana saved objects, so `shipped_saved_objects_only_query_provenance_names_the_templates_define`
    reports which provenance names they still query and fails if an alias is
    dropped while one depends on it. The retention half stays operational.

## 3. Output data-stream naming contract (ADR-0015)
- [x] 3.1 Ensure every processor-emitted stream name follows `{class}-{subtype}[.sub]-esdiag` (class ∈ `metrics | settings | logs | health`).
- [x] 3.2 Add a test reconciling the two ESDiag-owned layers: every emitted stream name has a matching index template and vice versa.
- [x] 3.3 Author/verify dashboards against the convention (review discipline; not derived).
  - Discipline had already slipped, and the dashboards now living in
    `assets/kibana/` made it checkable: four data views matched on a bare prefix
    (`settings-index*`, `settings-node*`, `metrics-task-*`,
    `metrics-ingest.pipeline-*`), which also matches indices ESDiag does not own.
    Retitled to the `-esdiag` streams and pinned by
    `shipped_data_views_follow_the_stream_naming_contract`. Not derived, but no
    longer unverified.

## 4. Verification
- [x] 4.1 Test manifest reads succeed for a `support-diagnostics` manifest and an older-ESDiag manifest (unknown fields ignored, ESDiag fields defaulted).
- [x] 4.2 Test legacy/absent `Product` infers `Unknown`, and that indicators refine it, with no manifest rewrite.
- [x] 4.3 Test `product` ⇄ `application` alias resolution in both directions across old and new index mappings.
  - The original test asserted this against a hand-written "old" mapping that
    nothing in the repository produced, so it could not fail for the reason it
    existed. It now runs both generations through the code that creates them: the
    new side from the shipped template, the old side through the patch
    `install_provenance_aliases` applies
    (`both_provenance_names_resolve_in_either_index_generation`).
- [x] 4.4 Confirm the processor ↔ index-template consistency test fails on injected drift.
- [x] 4.5 Confirm the delta spec scenarios in `specs/manifest-compatibility/spec.md` and `specs/indexed-data-model/spec.md` are covered.
