# Collection-definition reconciliation

ESDiag owns its per-product collection definitions
(`assets/<product>/sources.yml`); [`support-diagnostics`](https://github.com/elastic/support-diagnostics)
is a **reconciliation input**, not a runtime authority (ADR-0006). The
genuinely valuable upstream asset is its per-version compatibility knowledge
(and the OS-command catalogue), maintained on every release — we pull that in;
we do not mirror upstream shape or content verbatim.

## How

```sh
# report drift (CI-friendly, non-zero exit on changes)
scripts/reconcile_sources.py --support-diagnostics ../support-diagnostics --check

# apply the overlay
scripts/reconcile_sources.py --support-diagnostics ../support-diagnostics
```

The overlay is a **field-level merge**:

| owner | fields |
|---|---|
| upstream (refreshed) | `versions`, `extension`, `subdir`, `retry` |
| ESDiag (preserved) | `tags`, `source_weight`, `processing_weight`, `streamable`, `processable`, `required`, `dependencies`, `collect_dependencies` |

Upstream semver4j/NPM-dialect ranges are normalized into native Rust `semver`
form at this boundary, so the runtime resolves versions with stock
`semver::VersionReq` — there is no runtime compatibility shim.

Deliberate divergences (renames such as `internal_health` → `health_report`,
removals, ESDiag-only sources) are recorded in
`assets/<product>/sources-divergences.yml` and are never reverted by the
script.

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

**Owner:** _unassigned_ — assign a DRI and wire `--check` into CI or a
scheduled reminder tied to both release cadences.
