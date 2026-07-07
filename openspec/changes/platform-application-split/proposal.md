## Why

The `Product` enum flattens two orthogonal axes — the deployment **platform** and the
**application** component — into one field. It therefore cannot represent
Elasticsearch-on-ECK (both axes at once), and `SelfManaged` is unrepresentable
(implicit as "an application with no wrapper"). This is the foundational domain
correction the rest of the architecture review depends on. Rationale: **ADR-0001**.

## What Changes

- Split `Product` into a **required** `Platform` (`SelfManaged | ElasticCloudHosted |
  ECE | ECK | KubernetesPlatform | Unknown`) and an **optional** `Application`
  (`Elasticsearch | Kibana | Logstash | Agent`).
- `Platform` is total (every diagnostic has exactly one; a bare install is
  `SelfManaged`) and **best-effort detected** — from indicators such as a `syscalls`
  folder (⇒ `SelfManaged`) or a manifest `runner` of `ece` (⇒ `ECE`) — falling back to
  `Unknown` when provenance cannot be established.
- `Application` is optional: a platform's own data has `application: None`; the display
  label is `application` if present, else `platform`.
- **BREAKING (internal):** retire the untyped `diagnostic.orchestration` string in
  favor of the typed `diagnostic.platform`.
- `Platform` propagates to included (child) diagnostics as they are spawned.

## Capabilities

### New Capabilities

- _(none — this modifies existing capabilities)_

### Modified Capabilities

- `orchestration-metadata`: retype and rename `orchestration` → `platform`; make
  `Platform` a total set including `SelfManaged`/`Unknown` with best-effort detection;
  add an optional `Application` classification; add platform propagation to children.

## Impact

- **Core processing:** `data::Product` becomes `Platform` + `Application`; the
  `orchestration` derivation in `Processor::start` (mod.rs:420); the diagnostic
  manifest; ~90 `Product` call sites.
- **CLI & Web UI:** display-label logic (`application` else `platform`).
- **Indexed docs:** the `diagnostic.platform` / `diagnostic.application` envelope
  fields — the read/alias compatibility for historical indices is handled separately
  by `indexed-and-manifest-compat` (ADR-0014), not here.
- **Foundational:** unblocks changes 2–9 of the architecture-review series.
