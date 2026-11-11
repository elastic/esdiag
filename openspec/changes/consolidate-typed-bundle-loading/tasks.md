## 1. Preserve the typed receiver contract

- [x] 1.1 Map existing coverage to the design's compatibility cases for both archive adapters; verify a coverage inventory distinguishes typed reads from raw and streaming assertions and identifies only genuinely uncovered cases.
- [x] 1.2 Add missing deterministic typed-receiver cases for ordered valid candidates, whitespace-only fallback, malformed/truncated first-candidate failure, and final `MissingSource` classification; verify the cases assert parsed values or typed failures through the existing receiver interface using synthetic file and memory archives.
- [x] 1.3 Retain nested working-directory, legacy double-slash and scrubbed address coverage; verify the selected cases preserve source context, excluded files, original non-address values and explicit scrub disable without pinning incidental logs or error wording.

## 2. Concentrate archive loading

- [x] 2.1 Move shared candidate iteration, empty-input handling and scrub-aware typed deserialization into the archive module using the existing generic `ZipArchive<R>` storage shape; verify no new public trait/interface, whole-entry String/Value materialization, lock policy or allocation solely for delegation is introduced.
- [x] 2.2 Migrate file-backed and in-memory archive adapters to the shared implementation in one cutover and delete the duplicated implementation; verify adapter-specific opening-error handling, source-product initialization, nested work-directory state and normalized-file lifetime remain intact.
- [x] 2.3 Keep raw reads, directory reads and the shared streaming route outside this cutover; verify review of affected callers finds no changed contract or obsolete compatibility path.

## 3. Verify the changed path

- [x] 3.1 Run `cargo test --lib receiver::`, `cargo test --test raw_receiver_tests`, `cargo test --test scrubbed_normalization_tests`, and `cargo test --test logstash_archive_processing_tests` after the cutover; verify every selected deterministic gate passes and classify any environment prerequisite rather than substituting a narrower assertion.
- [x] 3.2 Run `cargo clippy --workspace --all-targets` after implementation is complete; verify no new lint finding remains in the affected implementation.
- [x] 3.3 Launch the actual CLI on a temporary synthetic Elasticsearch archive derived from `tests/archives/elasticsearch-api-diagnostics-9.3.3.zip`, with isolated `ESDIAG_HOME` and directory Export, once with `--scrubbed true` and once with `--scrubbed false`; verify fixture node IDs, normalized versus original addresses, preserved port/identifier semantics, expected stream-specific node documents and document-failure results. Remove only the generated smoke artifacts after recording observed results; do not commit customer diagnostics.
- [x] 3.4 Review nearby architecture documentation after smoke proof and update only descriptions made obsolete by consolidation; verify operator-facing commands/behavior and changelog remain intentionally unchanged for this behavior-preserving refactor, or separately identify an approved observable correction before changing them.

## 4. Review and complete the child change

- [x] 4.1 Run `OPENSPEC_TELEMETRY=0 openspec validate consolidate-typed-bundle-loading --strict` and review implementation against the design's compatibility cases; verify the active planning artifacts remain valid with `skip_specs: true` and no invented main-spec delta.
- [x] 4.2 Publish native layer 2 as PR437 in `elastic/esdiag`, with head `stack/pr330-typed-loading` and base `stack/pr330-scrubbing` (PR436). Verify the immediate-parent diff excludes inherited scrubbing changes, all head branches share the upstream repository, and GitHub records the four-layer stack. Keep this change active through implementation review.
- [ ] 4.3 Resolve stack review, propagate lower-layer fixes upward, re-verify the combined implementation, then archive this no-delta change with artifacts preserved. Verify native OpenSpec validation, required completion gates and linear ancestry before merging the top PR and its lower layers into upstream/main. Do not defer the simplifications or replay already inherited parent commits.
