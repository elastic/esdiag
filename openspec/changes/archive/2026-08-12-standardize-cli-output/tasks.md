## 1. Output Contract Foundation

- [x] 1.1 Replace `serde_yaml` with the `yaml_serde` crate from the maintained `yaml-serde` project in dependencies, lockfile, imports, tests, and persisted-data call sites without changing existing YAML fixtures.
- [x] 1.2 Add global `OutputFormat` parsing for `yaml` and `json`, defaulting to YAML independently of TTY and agent-mode detection.
- [x] 1.3 Implement one serializer boundary using pretty block YAML and compact JSON with deterministic field ordering and trailing newline behavior.
- [x] 1.4 Define internally tagged `CliOutcome` variants and reusable compact bundle, process, and send result structs for direct diagnostic commands and composite job outcomes, plus setup, host, keystore, and serve readiness operations.
- [x] 1.5 Define stable `CliFailure` categories and optional context fields, including HTTP response `status`, `type`, and `reason`, with tests proving CLI credentials and unrestricted error chains cannot serialize.
- [x] 1.6 Add golden YAML and JSON fixtures plus compatibility tests for discriminator, snake_case fields, omitted optionals, string scalars, counts, and `duration_ms`.

## 2. Diagnostic Workflow Outcomes

- [x] 2.1 Project `CollectionResult` into archive path, successful/total file counts, duration, and optional upload destination fields.
- [x] 2.2 Project `Completed` into primary diagnostic ID, product, document count, duration, source, and Kibana URL without serializing the full report.
- [x] 2.3 Project every `IncludedDiagnosticOutcome` completed, skipped, and failed variant into typed child entries with safe errors.
- [x] 2.4 Project the unified executor's composite `JobOutcome` into one job result containing every durable save, process, and send result without mislabeling temporary or loaded bundles.
- [x] 2.5 Extend unified executor errors with failed-stage identity and the accumulated outcome, then project allowlisted completed-stage facts plus retry safety into structured finite-command failures.
- [x] 2.6 Add exhaustive conversion tests for direct and composite stage combinations, partial failures, temporary bundles, loaded inputs, and absent optional links.

## 3. Remaining Command Outcomes

- [x] 3.1 Replace host add, update, remove, auth, and list prose/table rendering with safe typed outcomes and explicit empty lists.
- [x] 3.2 Replace keystore create, add, update, remove, status, unlock, lock, password, and migrate prose with safe typed outcomes.
- [x] 3.3 Replace saved-job list and delete rendering with typed outcomes that expose the phase-composed input/save/process/export/send model, safe identifiers, and empty collections without a retired action field.
- [x] 3.4 Add typed upload and setup outcomes with affected target and artifact facts that omit authentication material.
- [x] 3.5 Add structured `serve` readiness when stdout is free and retain stderr-only readiness when an exporter owns stdout.

## 4. CLI Emission And Failure Handling

- [x] 4.1 Change command dispatch to return `CliOutcome` for every finite command and serialize it exactly once after successful execution.
- [x] 4.2 Replace the top-level `main() -> Result` terminal path with explicit structured finite-command failures and conventional non-zero exit codes.
- [x] 4.3 Preserve Clap's human-readable help, version, and argument-error behavior before command execution.
- [x] 4.4 Keep tracing, warnings, debug details, and progress exclusively on stderr in normal, debug, and agent modes.
- [x] 4.5 Detect stdout-owned NDJSON workflows before execution, including `job run` definitions whose process export is stdout, and prevent YAML, JSON, prose summaries, or structured failures from being appended to a document stream.
- [x] 4.6 Add integration coverage proving stdout contains exactly one parseable value for finite success and failure paths while stderr logging cannot contaminate it.

## 5. Parser Removal And Documentation

- [x] 5.1 Update the canonical skill to consume typed configuration checks, diagnostic identifiers, Kibana links, archive paths, and included outcomes without prose matching.
- [x] 5.2 Delete `extract-diagnostic.sh` from canonical and generated skill assets and replace its fixtures with native YAML/JSON outcome assertions.
- [x] 5.3 Document default YAML, `--format json`, success and failure schemas, stdout/stderr ownership, stream exceptions, and migration from prose/table parsing.
- [x] 5.4 Update repository shell and end-to-end consumers to deserialize explicit fields and stop matching completion summaries.
- [x] 5.5 Update `CHANGELOG.md` using the repository changelog skill and cross-reference the successor onboarding and agent CLI changes for remaining script removal.

## 6. Verification

- [x] 6.1 Run formatting and targeted outcome, persistence, redaction, CLI, phase-composed job, host, keystore, setup, and NDJSON stream tests.
- [x] 6.2 Test all finite command families in both YAML and JSON, composite and partially failed job outcomes, saved-job stdout exports, empty list outcomes, agent-mode parity, and structured non-zero failures.
- [x] 6.3 Run `cargo clippy --all-targets --all-features -- -D warnings`.
- [x] 6.4 Run `cargo test --all-features` and relevant CLI/end-to-end suites.
- [x] 6.5 Verify `yaml_serde` is the only general YAML Serde implementation, the skill no longer includes `extract-diagnostic.sh`, and strict OpenSpec validation passes.
