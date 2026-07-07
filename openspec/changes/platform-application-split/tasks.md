# Tasks

## 1. Types
- [x] 1.1 Add `Platform` enum (`SelfManaged | ElasticCloudHosted | ECE | ECK | KubernetesPlatform | Unknown`) with `Display`/`FromStr`/serde (case-insensitive, matching existing `Product` conventions).
- [x] 1.2 Add `Application` enum (`Elasticsearch | Kibana | Logstash | Agent`) with the same conventions; model it as `Option<Application>`.
- [x] 1.3 Keep `Product` as a temporary legacy alias with `From`/`Into` to the new pair; mark it for removal and add no new variants.

## 2. Detection
- [x] 2.1 Replace the product→string derivation in `Processor::start` (`src/processor/mod.rs:420`) with a typed `Platform` detector.
- [x] 2.2 Implement indicator rules: manifest `runner` (`ece` ⇒ `ECE`, …), presence of a `syscalls` folder ⇒ `SelfManaged`, cloud markers ⇒ `ElasticCloudHosted`/`ECK`/`KubernetesPlatform`; default `Unknown`.
- [x] 2.3 Ensure every consumer tolerates `Platform::Unknown` without failing.

## 3. Manifest & envelope
- [x] 3.1 Add `platform` + optional `application` to the diagnostic manifest/metadata; remove the `orchestration` field.
- [x] 3.2 Emit `diagnostic.platform` and `diagnostic.application` in the indexed-doc envelope (metadata builder).
- [x] 3.3 Implement the display-label rule: `application` if present, else `platform`.

## 4. Propagation
- [x] 4.1 In `spawn_sub_processors`, set each child's `Platform` from the parent explicitly (typed), preserving the application-layer invariant for children.

## 5. Migration of call sites
- [x] 5.1 Migrate `Product` call sites (~90) to `Platform`/`Application`; remove the legacy alias once clear.
  > **Staged (see QUESTIONS.md):** the processor/report/manifest/display axes are fully
  > migrated. The remaining `Product` sites are the known-host `app` axis (KnownHost,
  > client/receiver dispatch, CLI host flags, server host forms) plus the legacy wire
  > `product` field on manifests (kept deliberately — manifests are additive-only per
  > ADR-0010). The host axis is blocked on a modeling decision — template hosts use
  > `Product::Unknown` as a placeholder and `Application` has no `Unknown`; cloud-admin
  > hosts are not applications — which change 9 (`product-acquisition-scope`, Collect
  > scoped to ES/Kibana/Logstash) resolves. The alias stays until then, per design
  > ("Keep `Product` temporarily as a legacy alias to stage the ~90-site migration").

## 6. Verification
- [x] 6.1 Unit tests for detection (each indicator → expected `Platform`; indeterminate → `Unknown`).
- [x] 6.2 Test platform-only (`application: None`) vs application diagnostics, and the display-label fallback.
- [x] 6.3 Test child inherits parent platform (ECK → ES child = `platform: ECK, application: Elasticsearch`).
- [x] 6.4 Confirm the delta spec scenarios in `specs/orchestration-metadata/spec.md` are covered.
