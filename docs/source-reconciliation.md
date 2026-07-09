---
type: Maintainer Guide
title: "Collection-definition reconciliation"
tags: [repository, reconciliation]
---

# Collection-definition reconciliation

ESDiag owns its per-product collection definitions
(`assets/<product>/sources.yml`); [`support-diagnostics`](https://github.com/elastic/support-diagnostics)
is a **reconciliation input**, not a runtime authority (ADR-0006). The
genuinely valuable upstream asset is its per-version compatibility knowledge,
plus the OS-command catalogue that will matter once ESDiag has command-source
collection. We reconcile the supported inputs without mirroring upstream shape
or content verbatim.

## How

```sh
# report drift (CI-friendly, non-zero exit on changes)
cargo run --bin reconcile-sources -- --support-diagnostics ../support-diagnostics --check

# apply the overlay
cargo run --bin reconcile-sources -- --support-diagnostics ../support-diagnostics
```

The overlay is a **field-level merge**:

| owner | fields |
|---|---|
| upstream (refreshed) | `versions`, `extension`, `subdir`, `retry` |
| ESDiag (preserved) | `tags`, `source_weight`, `processing_weight`, `streamable`, `processable`, `required`, `dependencies`, `collect_dependencies` |

The expected upstream layout has been verified against
`elastic/support-diagnostics`:

| product/input | upstream path |
|---|---|
| Elasticsearch REST APIs | `src/main/resources/elastic-rest.yml` |
| Kibana REST APIs | `src/main/resources/kibana-rest.yml` |
| Logstash REST APIs | `src/main/resources/logstash-rest.yml` |
| OS-command catalog | `src/main/resources/diags.yml` |

Today the reconciliation tool overlays the REST API files. It verifies `diags.yml` is present
so upstream layout drift is visible, but it does not merge OS-command entries
until ESDiag has a command-source transport model; adding those entries to the
HTTP registry now would make broad collection modes try to collect shell
commands as REST paths.

Upstream-backed REST sources are tagged during reconciliation so ESDiag bundles
stay compatible with support-diagnostics coverage. Elasticsearch and Logstash
sources default to `support`; Kibana sources default to `standard,light,support`
so Kibana standard/light collection remains full-catalog until curated subsets
exist.

Upstream semver4j/NPM-dialect ranges are normalized into native Rust `semver`
form at this boundary, so the runtime resolves versions with stock
`semver::VersionReq` — there is no runtime compatibility shim.

Deliberate divergences (renames such as `internal_health` → `health_report`,
removals, ESDiag-only sources) are recorded in
`assets/<product>/sources-divergences.yml` and are never reverted by the
tool.

After applying, run `cargo test` — the registry is validated at startup and in
tests (key alignment, native-semver ranges, required keys).

## Cadence and ownership

Reconciliation is a required, recurring discipline (ADR-0006), performed on:

- **every application release** — a new Elasticsearch / Kibana / Logstash
  version can add or change endpoints and their version gating;
- **every support-diagnostics release** — upstream may revise definitions or
  OS commands.

Without this cadence, version gating silently goes stale (new endpoints
missed, changed queries not updated) — the primary risk the
own-and-reconcile posture accepts.

**Owner:** ESDiag maintainers. The release DRI for each Elasticsearch, Kibana,
Logstash, or support-diagnostics release owns running `--check` until this is
backed by CI or a scheduled reminder tied to both release cadences.
