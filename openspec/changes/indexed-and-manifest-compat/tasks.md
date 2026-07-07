# Tasks

## 1. Manifest read-compatibility (ADR-0010)
- [ ] 1.1 Make the manifest / `DiagnosticManifest` deserialization tolerant: ignore unknown fields, mark all ESDiag-added properties `Option` / `#[serde(default)]`.
- [ ] 1.2 Confirm no existing interchange field is removed, renamed, or repurposed; document the additive-only constraint at the model.
- [ ] 1.3 Resolve legacy/absent `Product` on read by inference (default `Platform::Unknown`, refined by `syscalls` folder ⇒ `SelfManaged`, `runner: ece` ⇒ `ECE`), without rewriting the manifest. (Detector owned by `platform-application-split`.)

## 2. Indexed-data field aliases (ADR-0013, ADR-0014)
- [ ] 2.1 In the `esdiag@*` templates, add bidirectional field aliases so `diagnostic.product` and `diagnostic.application` resolve to the same underlying field.
- [ ] 2.2 Replace `diagnostic.orchestration` with `diagnostic.platform` in the templates with no alias.
- [ ] 2.3 Keep new/renamed envelope fields ECS-inspired but source-API-aligned; layer `diagnostic.*` / `cluster.*` over the source-shaped payload.
- [ ] 2.4 Record the transitional aliases as tracked debt with a removal trigger (dashboards migrated + old indices aged out).

## 3. Output data-stream naming contract (ADR-0015)
- [ ] 3.1 Ensure every processor-emitted stream name follows `{class}-{subtype}[.sub]-esdiag` (class ∈ `metrics | settings | logs | health`).
- [ ] 3.2 Add a test reconciling the two ESDiag-owned layers: every emitted stream name has a matching index template and vice versa.
- [ ] 3.3 Author/verify dashboards against the convention (review discipline; not derived).

## 4. Verification
- [ ] 4.1 Test manifest reads succeed for a `support-diagnostics` manifest and an older-ESDiag manifest (unknown fields ignored, ESDiag fields defaulted).
- [ ] 4.2 Test legacy/absent `Product` infers `Unknown`, and that indicators refine it, with no manifest rewrite.
- [ ] 4.3 Test `product` ⇄ `application` alias resolution in both directions across old and new index mappings.
- [ ] 4.4 Confirm the processor ↔ index-template consistency test fails on injected drift.
- [ ] 4.5 Confirm the delta spec scenarios in `specs/manifest-compatibility/spec.md` and `specs/indexed-data-model/spec.md` are covered.
