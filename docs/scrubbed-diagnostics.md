# Scrubbed diagnostic normalization

esdiag can repair deterministic malformed IPv4 values in scrubbed Elasticsearch API diagnostics before processors run. This keeps node address fields ingest-safe and improves node summary joins without changing processor logic.

## When normalization runs

Normalization runs in the **receiver read path** for archive and directory inputs (`process` CLI and upload UI). Processors always receive either original or already-normalized JSON.

| Channel | Control | Default when unset |
|---------|---------|-------------------|
| CLI `process` | `--scrubbed true\|false` | Auto: enabled when the input path contains `scrubbed` as a standalone token (zip **or** extracted directory) |
| Upload UI | `scrubbed` multipart field (checkbox) | Auto: enabled when the **uploaded** filename contains `scrubbed` as a standalone token (not the temp staging path) |
| Service link | _(no checkbox yet)_ | Auto: enabled when the **filename** field contains `scrubbed` as a standalone token |

Precedence is **channel-local**:

- CLI `--scrubbed true` forces normalization even if the filename does not contain a `scrubbed` token.
- CLI `--scrubbed false` disables normalization even for `*scrubbed*.zip` inputs.
- Upload checkbox values `true`, `1`, or `on` enable scrub mode; any other explicit value disables it. If the checkbox field is present but unreadable, scrub mode defaults to enabled.

CLI and UI are independent execution channels; one does not override the other.

## Supported files and fields

Normalization applies to address fields in **all diagnostic `.json` files** except `diagnostic_manifest.json` and `version.json` (including `nodes.json`, `nodes_stats.json`, `tasks.json`, `master.json`, `shards.json`, etc.).

Only explicit address fields are rewritten:

- Pure IP semantics (`ip` mapping and keyword mirrors): `ip`, `host`, `publish_host`, `bind_host`
- IP with optional port: `transport_address`, `publish_address`, `bound_address`, `local_address`, `remote_address`, `x_forwarded_for`
- HTTP client IDs under `http.clients[].id` when the value is a malformed dotted-quad

All other `.json` diagnostic files are scanned (except `diagnostic_manifest.json` and `version.json`).

## Normalization rules

- Malformed IPv4 octets use deterministic modulo: `octet % 255`.
- Valid IPv4 values (all octets `<= 255`) pass through unchanged.
- For `ip:port` fields, only the IP component is normalized; the port is preserved.
- For pure IP fields (`ip`, `host`), normalized output is IP-only (no port suffix).

Scrubbed 19-character lowercase hex node names are humanized during node lookup processing (for example `aaaabbbbccccddddee0` → `hot-dee0`).

## Customer diagnostics policy

**Never commit customer scrubbed API bundles to this repository.**

Automated tests build synthetic malformed IPs at runtime from the esdiag golden archive (`tests/archives/elasticsearch-api-diagnostics-9.1.3.zip`). See `tests/scrubbed_normalization_tests.rs` and the `synthetic_vectors` test module in `src/receiver/archive/scrub.rs` for canonical test constants.

## Deterministic verification (CI)

```bash
cargo test --lib scrub
cargo test --test scrubbed_normalization_tests
cargo test --test scrub_debug_log_tests
cargo clippy --workspace --all-targets
npx openspec validate normalize-malformed-scrubbed-ips
```

The integration test transforms the golden archive in a temp directory (malformed IP substitution), runs `process` → directory export, and asserts normalized node metrics — no committed scrubbed zip or helper scripts required.

## Dev ingest verification (environment-gated)

Optional before merge when you have a live Elasticsearch output target (`ESDIAG_OUTPUT_URL` in a `KEY=value` env file). Use a **local** archive whose filename contains `scrubbed`, or pass `--scrubbed true` explicitly. Do not commit customer bundles.

```bash
cargo build --release
set -a && source /path/to/.env && set +a

esdiag process /path/to/local-archive.zip --scrubbed true --debug

jq '.diagnostic | {doc_errors: .docs.errors, created: .docs.created, node_metrics: .processor.stats["metrics-node-esdiag"]}' \
  ~/.esdiag/last_run/report.json
```

Pass criteria:

1. `esdiag process` exits 0.
2. `diagnostic.docs.errors` is zero in `~/.esdiag/last_run/report.json`.
3. `diagnostic.processor.stats["metrics-node-esdiag"].docs` is non-zero when the archive includes node stats.

Recorded run: `openspec/changes/archive/2026-06-04-normalize-malformed-scrubbed-ips/validation-code-gates.md` (§3 Live ingest)

## Memory regression spot-check

Compare RSS for the same archive with scrub mode off vs on. Target: **≤ 20% RSS increase** with scrub enabled.

Linux:

```bash
/usr/bin/time -v ./target/release/esdiag process tests/archives/elasticsearch-api-diagnostics-9.1.3.zip --scrubbed false -o /tmp/esdiag-out-base
/usr/bin/time -v ./target/release/esdiag process tests/archives/elasticsearch-api-diagnostics-9.1.3.zip --scrubbed true -o /tmp/esdiag-out-scrub
```

macOS:

```bash
/usr/bin/time -l ./target/release/esdiag process tests/archives/elasticsearch-api-diagnostics-9.1.3.zip --scrubbed false -o /tmp/esdiag-out-base
/usr/bin/time -l ./target/release/esdiag process tests/archives/elasticsearch-api-diagnostics-9.1.3.zip --scrubbed true -o /tmp/esdiag-out-scrub
```

Use the `Maximum resident set size` line from each run.

## Troubleshooting

| Symptom | Likely cause | What to check |
|---------|--------------|---------------|
| Node metrics missing in Kibana | Time picker excludes collection date | Metric docs use manifest collection date for `@timestamp`, not ingest time |
| Scrub normalization did not run | Auto mode off or directory path without scrub wiring (fixed in feature branch) | Use `./target/release/esdiag` from this branch; pass `--scrubbed true`; prefer `*.zip` or ensure extracted folder name contains `scrubbed` |
| Valid IPs changed unexpectedly | Scrub mode forced on non-scrubbed bundle | Re-run with `--scrubbed false` |
| Partial node summary fields | Node lookup miss | Debug logs show `Node lookup not found`; fallback summary patch should still populate `node.id` and tier fields |

Enable debug logging to see per-file normalization:

```bash
esdiag process scrubbed-api-diagnostics.zip --scrubbed true --debug
```

Look for lines like `Unscrubbed N address fields in .../nodes.json`.

## Test matrix

| Gate | Scope | Command |
|------|-------|---------|
| Deterministic unit tests | Scrub helpers, receiver wiring, upload parsing, node rename | `cargo test --lib scrub` |
| Deterministic integration | In-process `process` → directory export; per-node IP mapping across NDJSON streams; archive **and** directory paths; scrub disabled regression | `cargo test --test scrubbed_normalization_tests` |
| Debug log assertions | Scrub mode context + per-file unscrubbed counts | `cargo test --test scrub_debug_log_tests` |
| Workspace lint | Changed Rust sources | `cargo clippy --workspace --all-targets` |
| OpenSpec | Change artifacts | `npx openspec validate normalize-malformed-scrubbed-ips` |
| Environment-gated ingest | Live Elasticsearch target | Manual `esdiag process --debug` + `report.json` checks (see above) |
| Environment-gated full suite | Known-host / container dependent tests | `cargo test --workspace` (classify pre-existing failures) |
