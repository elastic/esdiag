# Tasks

## 1. Types
- [ ] 1.1 Add `Platform` enum (`SelfManaged | ElasticCloudHosted | ECE | ECK | KubernetesPlatform | Unknown`) with `Display`/`FromStr`/serde (case-insensitive, matching existing `Product` conventions).
- [ ] 1.2 Add `Application` enum (`Elasticsearch | Kibana | Logstash | Agent`) with the same conventions; model it as `Option<Application>`.
- [ ] 1.3 Keep `Product` as a temporary legacy alias with `From`/`Into` to the new pair; mark it for removal and add no new variants.

## 2. Detection
- [ ] 2.1 Replace the product→string derivation in `Processor::start` (`src/processor/mod.rs:420`) with a typed `Platform` detector.
- [ ] 2.2 Implement indicator rules: manifest `runner` (`ece` ⇒ `ECE`, …), presence of a `syscalls` folder ⇒ `SelfManaged`, cloud markers ⇒ `ElasticCloudHosted`/`ECK`/`KubernetesPlatform`; default `Unknown`.
- [ ] 2.3 Ensure every consumer tolerates `Platform::Unknown` without failing.

## 3. Manifest & envelope
- [ ] 3.1 Add `platform` + optional `application` to the diagnostic manifest/metadata; remove the `orchestration` field.
- [ ] 3.2 Emit `diagnostic.platform` and `diagnostic.application` in the indexed-doc envelope (metadata builder).
- [ ] 3.3 Implement the display-label rule: `application` if present, else `platform`.

## 4. Propagation
- [ ] 4.1 In `spawn_sub_processors`, set each child's `Platform` from the parent explicitly (typed), preserving the application-layer invariant for children.

## 5. Migration of call sites
- [ ] 5.1 Migrate `Product` call sites (~90) to `Platform`/`Application`; remove the legacy alias once clear.

## 6. Verification
- [ ] 6.1 Unit tests for detection (each indicator → expected `Platform`; indeterminate → `Unknown`).
- [ ] 6.2 Test platform-only (`application: None`) vs application diagnostics, and the display-label fallback.
- [ ] 6.3 Test child inherits parent platform (ECK → ES child = `platform: ECK, application: Elasticsearch`).
- [ ] 6.4 Confirm the delta spec scenarios in `specs/orchestration-metadata/spec.md` are covered.
