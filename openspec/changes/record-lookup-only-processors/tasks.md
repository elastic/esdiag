## 1. Core Structure Updates

- [x] 1.1 Add `parsed: bool` to `Lookup<T>` in `src/processor/diagnostic/lookup.rs`
- [x] 1.2 Add `parsed: bool` to `LookupSummary` in `src/processor/diagnostic/report.rs`
- [x] 1.3 Update `DiagnosticReport::add_lookup` to record `parsed` status and track failures

## 2. Integration

- [x] 2.1 Update all lookup data source `From` implementations to call `.was_parsed()` on success

## 3. Verification

- [x] 3.1 Run `cargo clippy` on the updated modules
- [x] 3.2 Add unit tests to verify `parsed` status and failure recording in the report's lookup section
- [x] 3.3 Run `cargo test` to ensure all tests pass
