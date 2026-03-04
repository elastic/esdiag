## 1. Snapshot Module and Data Source Wiring

- [x] 1.1 Ensure `src/processor/elasticsearch/snapshots/` is registered in `src/processor/elasticsearch/mod.rs`.
- [x] 1.2 Keep separate datasource types for repositories and snapshots, with endpoint mappings for `_snapshot` and `_snapshot/_all/_all`.

## 2. Streaming Snapshot Data Path

- [x] 2.1 Implement `StreamingDataSource` for `Snapshots` using `indices_stats`-style incremental deserialization.
- [x] 2.2 Ensure deserialization emits per-snapshot items without full payload materialization.
- [x] 2.3 Add streaming deserialization tests for normal payloads and early channel close behavior.

## 3. Streaming Snapshot Export Path

- [x] 3.1 Implement `StreamingDocumentExporter` for `Snapshots` following `indices_stats` channel-based document export.
- [x] 3.2 Use bounded channel buffers and batched sends for snapshot documents.
- [x] 3.3 Ensure channel closure/join behavior is handled and processor summaries are merged.
- [x] 3.4 Verify per-entry failures are logged and do not abort remaining stream processing.

## 4. Structured Document Contract

- [x] 4.1 Define and enforce stable core fields for snapshot documents (snapshot name, repository, state, contents, timing).
- [x] 4.2 Extract `snapshot.date` from a `YYYY.MM.DD` token in snapshot names when present.
- [x] 4.3 Add tests for `snapshot.date` extraction present/absent cases.
- [x] 4.4 Define and enforce stable core fields for repository documents (name, type, settings payload).
- [x] 4.5 Add tests validating exported document shape for both data streams.

## 5. Pipeline Integration

- [x] 5.1 Ensure `ElasticsearchDiagnostic` uses streaming processing (`process_streaming_datasource`) for snapshots.
- [x] 5.2 Keep repository processing integrated and compatible with current pipeline behavior.

## 6. Verification

- [x] 6.1 Run `cargo clippy`.
- [x] 6.2 Run `cargo test` including new snapshot streaming and export tests.
- [x] 6.3 Validate diagnostic output in a representative large-snapshot fixture or environment.

## 7. Assets

- [x] 7.1 Ensure `assets/elasticsearch/index_templates/settings-repository.json` exists and maps stable repository fields.
- [x] 7.2 Ensure `assets/elasticsearch/index_templates/logs-snapshot.json` exists and maps stable snapshot fields including `snapshot.date`.
