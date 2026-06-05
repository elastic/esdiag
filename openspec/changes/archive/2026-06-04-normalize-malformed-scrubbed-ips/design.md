## Context

Scrubbed diagnostic archives can contain deterministic but malformed IPv4 values and anonymized node names that break node-level readability and cross-source joins. In practice, this causes partial dashboards (especially node summary views) even when ingest technically succeeds.

esdiag already has a receiver -> processor -> exporter architecture, and this change fits best as a receiver-stage normalization pass so processors can remain focused on business logic rather than scrub-specific repair logic.

Constraints and decisions from proposal review:
- receiver-first handling only (no ingest retry fallback path)
- scrub mode auto-detect by filename/path containing `scrubbed`
- explicit control in each execution channel (CLI flag in CLI path, checkbox in UI path)
- UI/CLI are treated as either/or channels rather than cross-overriding controls
- normalization only on explicit address fields (no free-text global rewrite)
- non-scrubbed bundles must not be mangled
- scrubbed node names are 19-char lowercase hex and should be humanized using last 4 chars

## Goals / Non-Goals

**Goals:**
- Detect scrubbed archives early in receiver flow and normalize before processor parsing.
- Normalize malformed IPv4-like values deterministically using octet `% 255`.
- Preserve document usability and processor joins by standardizing node address/name surfaces.
- Humanize scrubbed node names using existing rename logic shape with last-4 suffix behavior.
- Guarantee non-mangling of non-scrubbed diagnostics through fixture-based tests.
- Keep memory overhead bounded with a target of <= 20% RSS increase vs non-scrubbed processing.

**Non-Goals:**
- No ingest-pipeline retry/fallback mechanism for malformed IP failures.
- No global free-text replacement outside explicit address/name fields.
- No external dependencies or sidecar preprocessing service.
- No change to core processor state-machine design.

## Decisions

### 0) Test Strategy Must Match esdiag Test Topology

esdiag testing is intentionally mixed:
- deterministic unit/in-process tests (always runnable in CI/dev)
- environment-dependent integration tests (known-host aliases, local/remote services, containers)

Therefore this change uses layered verification instead of a single "all workspace tests must pass everywhere" gate.

Required per-phase gates:
- `cargo clippy --workspace --all-targets`
- targeted deterministic tests for touched modules (receiver/server/processor units)
- `openspec validate normalize-malformed-scrubbed-ips`

Environment-gated checks (required before merge, but not required on every local iteration):
- full `cargo test --workspace` with documented baseline for existing env-dependent failures
- live ingest validation against Elasticsearch dev target using `esdiag process --debug`
- post-run artifact checks in `~/.esdiag/last_run`

### 1) Receiver-Stage Scrub Normalization

Implement a scrub-aware normalization layer in receiver archive readers (`archive/file` and `archive/bytes`) that transforms selected file payloads before processor consumption.

Rationale:
- Centralized behavior for all downstream processors.
- Avoids duplicated repair logic across `nodes`, `nodes_stats`, `tasks`, and other processors.
- Preserves existing type-state processor flow with minimal architectural disruption.

Alternative considered:
- Processor-side normalization only. Rejected because it fragments behavior across modules and is easy to miss when data is represented as `serde_json::Value`.

### 2) Activation Model and Precedence

Activation modes:
- CLI channel:
  - implicit `auto`: when `--scrubbed` is not provided, enable only if input archive filename/path contains `scrubbed`
  - explicit override: `--scrubbed true` forces scrub normalization
  - explicit override: `--scrubbed false` forces no scrub normalization
- UI channel:
  - checkbox controls scrub behavior for upload path
  - auto-detect fallback applies when checkbox is unset

Precedence:
- resolve precedence within channel only
- no cross-channel precedence is required because execution is either CLI or UI

Surface:
- CLI flag `--scrubbed BOOL` (for `process` and server ingest path wiring)
- Upload UI checkbox mapped to same mode enum

Rationale:
- Safe default for known scrubbed archives plus explicit operator control for edge cases.

### 3) Explicit Field Allowlist

Normalization SHALL apply only to explicit address fields:
- `ip`, `host`
- `transport_address`, `publish_address`, `bound_address`
- known nested transport/http address fields (`local_address`, `remote_address`, `x_forwarded_for`)

Rationale:
- Reduces accidental mutation risk and supports non-mangling guarantee.

Alternative considered:
- Rewrite any string matching malformed IPv4 pattern. Rejected as too risky for semantic text fields.

### 4) Deterministic IPv4 Rewrite

Malformed dotted-quad octets are normalized via `octet % 255` per octet.

For `ip:port` fields:
- normalize IP component only
- preserve original port component

For fields typed as pure IP semantics (`ip`, `host`):
- store normalized IP only (no port suffix)

Rationale:
- Stateless, deterministic, low-compute transformation requested in proposal discussion.

### 5) Node Name Humanization Rule

For scrubbed node names matching 19-char lowercase hex (example: `c4e8f2a16b3d4f099e7`):
- use existing tier rename logic shape
- replace numeric segment with original scrubbed name last 4 chars

Example behavior:
- source: `c4e8f2a16b3d4f099e7`, tier `hot`
- output style: `hot-99e7`

Rationale:
- Operator-readable labels while retaining deterministic traceability.

### 6) Type-State and Trait Boundaries

No processor lifecycle changes required. Existing transitions remain:
- `Processor<Ready> -> Processor<Processing> -> Processor<Completed|Failed>`

Receiver boundary updates:
- Introduce scrub-mode decision and transformation inside receiver archive read path.
- Keep `Receiver` trait contract intact by returning normalized bytes/content transparently.
- Follow existing logging pattern (`log::debug!`) and emit one debug line for each file read that is unscrubbed.

## Risks / Trade-offs

- **[False positive auto-detect on filename]** -> Mitigation: manual override always wins (`on`/`off`).
- **[Unexpected field mutation]** -> Mitigation: strict allowlist + golden non-mangling fixtures.
- **[Join regressions across node datasets]** -> Mitigation: add integration assertions for node ID/name/address consistency in `nodes` + `nodes_stats`.
- **[Memory overhead from transformation]** -> Mitigation: streaming/targeted transforms and RSS budget gate (<=20% increase).
- **[Inconsistent memory measurement across developer platforms]** -> Mitigation: document and use `/usr/bin/time -l` on macOS and `/usr/bin/time -v` on Linux in validation workflow.
- **[False-negative quality signal from environment-dependent tests]** -> Mitigation: classify tests into deterministic vs environment-gated; require deterministic gates every phase and run env-gated matrix before merge.
- **[Octet modulo collisions]** -> Mitigation: acceptable by design for scrubbed mode; deterministic output and logging for traceability.

