## 1. Agent mode CLI behavior

- [x] 1.1 Add a global `--agent` / `-a` CLI flag in `src/main.rs` and resolve agent mode at startup from the flag plus `CLAUDECODE`.
- [x] 1.2 Update tracing initialization so agent mode uses warn-level logging by default while `--debug` continues to force debug logging.
- [x] 1.3 Add shared tracing-independent `STDERR` completion helpers and use them for successful top-level CLI command results, including process summaries, without contaminating streamed `STDOUT`.

## 2. Viewer-aware Kibana link resolution

- [x] 2.1 Implement a helper that resolves the process command's Kibana base URL from an explicit saved `send` host's `viewer` reference, with `ESDIAG_KIBANA_URL` and `ESDIAG_KIBANA_SPACE` fallback behavior preserved.
- [x] 2.2 Thread the resolved optional Kibana base URL through the processing path and update final report link generation to use that resolved value instead of direct environment lookup.
- [x] 2.3 Add or update tests covering saved-viewer resolution, environment fallback, and the no-link case when neither a saved viewer host nor `ESDIAG_KIBANA_URL` is available.

## 3. Verification

- [x] 3.1 Add CLI regression tests for explicit `--agent`, `CLAUDECODE` auto-enable, warn-level suppression of info-only completion output, and final `STDERR` summaries while preserving streamed `STDOUT`.
- [ ] 3.2 Run `cargo clippy`.
- [ ] 3.3 Run `cargo test`.
