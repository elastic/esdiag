## 1. Data Model Updates

- [x] 1.1 Add `parent_id` and `orchestration` fields to `DiagnosticMetadata` in `src/processor/diagnostic/doc.rs`
- [x] 1.2 Add `parent_id` field to `DiagnosticManifest` in `src/processor/diagnostic/diagnostic_manifest.rs`
- [x] 1.3 Update `DiagnosticManifest::new` and related constructors to handle `parent_id`

## 2. Logic Implementation

- [x] 2.1 Implement orchestration mapping logic in `TryFrom<DiagnosticManifest> for DiagnosticMetadata`
- [x] 2.2 Update processor logic to propagate `parent_id` to included diagnostics
- [x] 3.1 Run `cargo clippy` to ensure code quality
- [x] 3.2 Run `cargo test` to verify no regressions
- [x] 3.3 (Optional) Add unit tests for parent-child relationship propagation
