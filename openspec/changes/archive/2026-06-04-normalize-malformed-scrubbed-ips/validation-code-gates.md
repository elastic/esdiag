# Code validation gates — normalize-malformed-scrubbed-ips

**Date:** 2026-06-02 (refreshed for final PR state)  
**Host:** WSL2 (Linux)  
**Branch:** `asa/normalize-malformed-ips-openspec`  
**Repo:** `/home/asa/repos/test-esdiag/esdiag`  
**Env file:** `/home/asa/repos/ya-esdiag/esdiag/.env` (`ESDIAG_OUTPUT_URL=http://localhost:9201`)

---

## 1. One-time setup

| Step | Result |
|------|--------|
| `cargo build --release` | **Pass** (1m 54s) |
| `jq` | Present (`/usr/bin/jq`) — apt install skipped |
| `time` | Present (shell builtin / `/usr/bin/time`) — apt install skipped |
| ES reachability | **Pass** — `curl http://localhost:9201` → **401** (host up; auth required) |
| Pre-commit credential scan | **Pass** — `git diff --cached \| grep …` → clean |

---

## 2. Deterministic gates (CI-safe)

### Commands

| Command | Result |
|---------|--------|
| `cargo test --lib scrub` | **15 passed**, 0 failed |
| `cargo test --test scrubbed_normalization_tests` | **6 passed**, 0 failed |
| `cargo test --test scrub_debug_log_tests` | **1 passed**, 0 failed |
| `cargo clippy --workspace --all-targets` | **Pass** (exit 0); 1 pre-existing `type_complexity` warning in `src/server/mod.rs:298`; 1 non-blocking `collapsible_if` in `tests/scrub_normalization_assertions.rs` |
| `npx openspec validate normalize-malformed-scrubbed-ips` | **Pass** — change is valid |

**Total deterministic scrub gates:** **22 tests** (15 unit + 6 integration + 1 debug-log).

### Unit test coverage (`cargo test --lib scrub`)

| Area | Tests |
|------|-------|
| Scrub engine (`receiver/archive/scrub.rs`) | Valid IPv4 pass-through; allowlisted fields only; `publish_host` / `bind_host`; `tasks.json`; other diagnostic JSON files; excluded manifest/version |
| Archive receiver (`receiver/archive/file.rs`) | Scrubbed zip normalizes `nodes.json`; non-scrubbed golden archive unchanged |
| Scrub mode resolution (`receiver/mod.rs`) | Auto (filename contains `scrubbed`); explicit true; explicit false |
| Upload checkbox parsing (`server/file_upload.rs`) | Truthy (`true`, `1`, `on`); falsy values |
| Node rename (`nodes/lookup.rs`) | 19-char lowercase hex → tier + last-4 suffix |

### Integration test coverage (`cargo test --test scrubbed_normalization_tests`)

Synthetic malformed IPs are injected at runtime from the golden archive (`tests/archives/elasticsearch-api-diagnostics-9.1.3.zip`); **no committed customer scrubbed bundle**.

| Test | What it proves |
|------|----------------|
| `archive_export_normalizes_ips_with_stable_node_mapping` | Zip input + explicit scrub → directory export; **two-node** fixture; per-`node.id` IP mapping across **10 NDJSON streams** |
| `directory_export_normalizes_ips_with_stable_node_mapping` | **Extracted folder** input + explicit scrub → same mapping assertions |
| `directory_auto_detect_enables_scrub_when_path_contains_scrubbed` | Folder path containing `scrubbed` enables normalization without `--scrubbed true` |
| `directory_with_scrub_disabled_preserves_malformed_ips` | `--scrubbed false` regression — malformed IPs pass through |
| `processes_non_scrubbed_golden_archive_without_error` | Golden archive with scrub off — no mangling |
| `distinct_malformed_ips_normalize_to_distinct_valid_ips` (assertions module) | Modulo transform produces distinct valid IPv4 per node index |

Shared assertions (`tests/scrub_normalization_assertions.rs`) verify:

- `node.host`, `node.ip`, `node.transport_address` on every applicable export line
- `node.settings.network.publish_host` on `settings-node-esdiag.ndjson` when value is a dotted quad
- Each `node.id` maps to **exactly one** normalized IP across streams (≥2 nodes, distinct expected IPs)

### Implementation scope validated by tests

| Surface | Covered |
|---------|---------|
| Input types | Archive (zip) **and** directory (extracted folder) |
| Scrub activation | CLI explicit flag, path auto-detect, upload filename hint (unit-tested); directory auto-detect (integration) |
| JSON files | All `*.json` except `diagnostic_manifest.json` and `version.json` |
| Address fields | `ip`, `host`, `publish_host`, `bind_host`, `transport_address`, `publish_address`, `bound_address`, `local_address`, `remote_address`, `x_forwarded_for`, malformed `http.clients[].id` |
| Non-goals | No rewrite of hyphenated K8s hostnames (`ip-10-36-…`); no global free-text replace |

Operator reference: `docs/scrubbed-diagnostics.md`.

---

## 3. Live ingest (env-gated, manual)

Deterministic coverage (no live ES): `cargo test --test scrubbed_normalization_tests` — builds synthetic malformed-IP zip from golden archive at runtime, exports to a temp directory, and asserts cross-stream node IP mapping.

Optional live Elasticsearch check (recorded 2026-06-02):

```bash
cd /home/asa/repos/test-esdiag/esdiag
cargo build --release
set -a && source /home/asa/repos/ya-esdiag/esdiag/.env && set +a

# Use ./target/release/esdiag from this branch (not a global install without --scrubbed).
# Enable scrub via --scrubbed true OR a filename/path containing "scrubbed" (auto mode).
./target/release/esdiag process /path/to/local-archive.zip --scrubbed true --debug

jq '.diagnostic | {doc_errors: .docs.errors, created: .docs.created, node_metrics: .processor.stats["metrics-node-esdiag"]}' \
  ~/.esdiag/last_run/report.json
```

Recorded run used a **local synthetic zip** (not committed; same golden→malformed transform as the integration test). Scrub was enabled via **auto-detect** (filename contained `scrubbed`); `--scrubbed true` would work without that.

**Result: PASS**

| Check | Outcome |
|-------|---------|
| `esdiag process` exit code | **0** |
| Bulk error artifact (`~/.esdiag/last_run/bulk_errors.ndjson`) | **empty / absent** |
| Scrub mode | **Enabled** (auto-detect from filename on recorded run) |
| `nodes.json` normalization | **5** address fields |
| `nodes_stats.json` normalization | **133** address fields (stream) |
| `diagnostic.docs.errors` | **0** |
| `metrics-node-esdiag` docs | **371** |
| Total documents created | **375** |
| Process runtime | **0.141 s** |
| Diagnostic ID | `esdiag-cluster@2025-09-18~2784` |
| Cluster / version | `esdiag-cluster` / 9.1.3 |
| esdiag version | `0.15.0-SNAPSHOT` |

Debug log excerpts:

```
Unscrubbed 5 address fields in api-diagnostics-20250918-001807/nodes.json
Unscrubbed 133 address fields in stream file api-diagnostics-20250918-001807/nodes_stats.json
Processor: metrics-node-esdiag  parsed: true  371 docs  0 errors
Created 375 documents for Elasticsearch diagnostic: esdiag-cluster@2025-09-18~2784
```

Full log: `/tmp/esdiag-live-ingest-validation-20260602.log`

Notes:

- Missing APIs in the minimal synthetic fixture produce expected `File not found in archive` warnings; they do not fail ingest.
- No committed scrubbed zip or shell helpers in this change.
- **Recommended before merge (not yet recorded here):** one manual run against a **local customer scrubbed API zip** with `--scrubbed true`, then spot-check exports for zero malformed octets in `node.ip`, `node.host`, `transport_address`, and `settings.network.publish_host`.

---

## 4. Memory spot-check (WSL / Linux)

Directory output (`-o /tmp/...`), golden archive `tests/archives/elasticsearch-api-diagnostics-9.1.3.zip`:

```bash
/usr/bin/time -v ./target/release/esdiag process \
  tests/archives/elasticsearch-api-diagnostics-9.1.3.zip \
  --scrubbed false -o /tmp/esdiag-out-base 2>&1 | tee /tmp/esdiag-mem-base.txt

/usr/bin/time -v ./target/release/esdiag process \
  tests/archives/elasticsearch-api-diagnostics-9.1.3.zip \
  --scrubbed true -o /tmp/esdiag-out-scrub 2>&1 | tee /tmp/esdiag-mem-scrub.txt
```

| Run | Maximum resident set size |
|-----|---------------------------|
| Baseline (`--scrubbed false`) | **125,556 kB** (~123 MiB) |
| Scrub enabled (`--scrubbed true`) | **124,188 kB** (~121 MiB) |

**Ratio:** scrub / baseline = **98.9%** (target ≤ **120%**) → **PASS**

Logs: `/tmp/esdiag-mem-base.txt`, `/tmp/esdiag-mem-scrub.txt`

Note: Golden archive has no malformed scrubbed IPs; scrub path adds negligible RSS on that fixture. Heavier normalization work is exercised by `scrubbed_normalization_tests` (runtime synthetic zip from golden, two-node mapping).

---

## Summary

| Gate category | Status |
|---------------|--------|
| Setup + ES reachability | **Pass** |
| Deterministic tests (22) + clippy + openspec | **Pass** |
| Live synthetic ingest | **Pass** |
| Memory RSS ≤ 120% baseline | **Pass** (98.9%) |
| Live customer scrubbed bundle smoke | **Recommended, not recorded** |

**Not in scope for this run:** `cargo test --workspace` / `collection_tests` (requires local `elasticsearch-local` collect setup — classify pre-existing env-gated failures separately).
