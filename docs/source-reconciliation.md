---
type: Maintainer Guide
title: Collection definition reconciliation
tags: [repository, reconciliation]
---

# Collection definition reconciliation

ESDiag owns `assets/<product>/sources.yml`. It reads
[`support-diagnostics`](https://github.com/elastic/support-diagnostics) to
update version support and request details. It does not load upstream files at
runtime. ADR-0006 explains why.

## Run the reconciler

Check for drift:

```sh
cargo run --bin reconcile-sources -- \
  --support-diagnostics ../support-diagnostics --check
```

Apply the upstream fields:

```sh
cargo run --bin reconcile-sources -- \
  --support-diagnostics ../support-diagnostics
```

Run `cargo test` afterward. The tests validate source keys, version ranges, and
required fields.

## What the tool changes

The tool updates these fields from upstream:

| Field | Source |
|---|---|
| `versions` | Upstream compatibility range |
| `extension` | Upstream request suffix |
| `subdir` | Upstream bundle path |
| `retry` | Upstream retry setting |

It keeps ESDiag-owned fields:

```text
tags
source_weight
processing_weight
streamable
processable
required
dependencies
collect_dependencies
```

The reconciler reads these upstream files:

| Product or input | Path |
|---|---|
| Elasticsearch REST APIs | `src/main/resources/elastic-rest.yml` |
| Kibana REST APIs | `src/main/resources/kibana-rest.yml` |
| Logstash REST APIs | `src/main/resources/logstash-rest.yml` |
| OS commands | `src/main/resources/diags.yml` |

It updates the three REST API files. It only checks that `diags.yml` exists.
ESDiag does not collect shell commands yet, so merging them into the HTTP
registry would make normal collection try to call shell commands as REST paths.

The tool adds collection tags for upstream REST sources:

- Elasticsearch and Logstash default to `support`.
- Kibana defaults to `standard,light,support`.

It converts upstream semver4j and NPM-style ranges to Rust `semver` ranges.
The runtime then uses `semver::VersionReq` directly.

## Intentional differences

Keep deliberate differences in
`assets/<product>/sources-divergences.yml`. Examples include renamed endpoints,
removed upstream sources, and ESDiag-only sources. The reconciler does not
overwrite that file.

## When to run it

Run the drift check for every ESDiag release and every
support-diagnostics release. New product versions can change endpoints or
version ranges. If nobody runs the check, those changes stay unnoticed.

The ESDiag release owner runs the check until CI or a scheduled reminder takes
over.
