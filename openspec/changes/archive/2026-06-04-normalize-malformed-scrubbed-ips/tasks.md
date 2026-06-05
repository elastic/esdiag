## 1. Receiver Scrub Mode Plumbing

- [x] 1.1 Add scrub mode decision model (implicit auto + explicit boolean override) and wire it through receiver construction path.
- [x] 1.2 Add CLI flag `--scrubbed BOOL` in `process` flow and propagate to receiver.
- [x] 1.3 Add upload-path checkbox mapping to scrub mode in server file upload flow.
- [x] 1.4 Implement precedence rules within each channel (CLI or UI) with no cross-channel override dependency.
- [x] 1.5 Add validation tests for channel-specific mode resolution (CLI-only and UI-only flows).
- [x] 1.6 Ensure CLI behavior is fully operable in headless workflows via `--scrubbed true|false`.
- [x] 1.7 Run `cargo clippy --workspace --all-targets` and fix warnings for changed files.
- [x] 1.8 Run targeted deterministic tests for touched modules (receiver/server/main CLI parsing) and record results.
- [x] 1.9 Run `openspec validate normalize-malformed-scrubbed-ips`.

## 2. Receiver-Stage Normalization Engine

- [x] 2.1 Implement scrubbed archive auto-detection using filename/path contains `scrubbed` when mode is `auto`.
- [x] 2.2 Implement deterministic malformed IPv4 normalization (`octet % 255`) helpers.
- [x] 2.3 Implement allowlist-only field traversal for address normalization in supported JSON files.
- [x] 2.4 Implement `ip`/`host` pure-IP behavior and `ip:port` normalization with port preservation in transport fields.
- [x] 2.5 Integrate normalization into archive read path (`archive/file` and `archive/bytes`) before processor consumption.
- [x] 2.6 Run `cargo clippy --workspace --all-targets` and fix warnings for changed files.
- [x] 2.7 Run targeted deterministic normalization tests and receiver integration tests.
- [x] 2.8 Run `openspec validate normalize-malformed-scrubbed-ips`.

## 3. Node Name Humanization

- [x] 3.1 Update node rename logic in `nodes/lookup` to detect 19-char lowercase hex scrubbed names.
- [x] 3.2 Implement default rename using existing shape but replace numeric segment with source last 4 chars.
- [x] 3.3 Preserve existing `instance-...` rename behavior unchanged.
- [x] 3.4 Add tests for fictional 19-char hex scrubbed names producing last-4 suffix behavior.
- [x] 3.5 Run `cargo clippy --workspace --all-targets` and fix warnings for changed files.
- [x] 3.6 Run targeted deterministic rename tests and lookup integration tests.
- [x] 3.7 Run `openspec validate normalize-malformed-scrubbed-ips`.

## 4. Non-Mangling and Integration Validation

- [x] 4.1 Add golden fixtures for non-scrubbed archives and assert unchanged pass-through.
- [x] 4.2 Add scrubbed fixtures with malformed IPs and assert normalized values on allowed fields only.
- [x] 4.3 Add integration assertion that node summary-relevant documents include expected node fields post-normalization.
- [x] 4.4 Add memory regression check for scrubbed processing and verify <=20% RSS increase vs non-scrubbed baseline.
- [x] 4.5 Add debug logging assertions that each unscrubbed file read emits a debug log line and mode context (`tests/scrub_debug_log_tests.rs`).
- [x] 4.6 Document dev ingest validation workflow: manual `esdiag process --debug` into Elasticsearch and `~/.esdiag/last_run` checks for zero bulk conflicts.
- [x] 4.7 Add dataset sanity assertions in dev ingest validation (non-empty `metrics-node-esdiag` and related node datasets).
- [x] 4.8 Document memory measurement commands in validation docs: `/usr/bin/time -l` on macOS and `/usr/bin/time -v` on Linux.
- [x] 4.9 Run `cargo clippy --workspace --all-targets`.
- [x] 4.10 Run deterministic suite + targeted integration tests for this feature.
- [x] 4.11 Run `cargo test --workspace` and classify failures as:
  - expected environment-gated (known-host/local service/container dependent), or
  - regression introduced by this change.
- [x] 4.12 Fail the stage on any new regression; do not fail solely on pre-existing environment-gated failures.
- [x] 4.13 Run `openspec validate normalize-malformed-scrubbed-ips`.

## 5. Rollout and Documentation

- [x] 5.1 Document `--scrubbed BOOL` and UI checkbox behavior, including precedence.
- [x] 5.2 Document either/or channel model (CLI flow vs UI flow) and channel-specific control rules.
- [x] 5.3 Document supported normalized fields and non-goals (no global free-text rewrite).
- [x] 5.4 Document dev ingest verification workflow in debug mode and pass/fail criteria for clean ingest.
- [x] 5.5 Document platform-specific memory measurement commands for regression checks.
- [x] 5.6 Add troubleshooting notes for scrub detection and per-file debug logs.
- [x] 5.7 Document the test matrix explicitly (deterministic gates vs environment-gated integration checks).
- [x] 5.8 Run `cargo clippy --workspace --all-targets`.
- [x] 5.9 Run deterministic test gates and final `openspec validate normalize-malformed-scrubbed-ips`.
