## 1. CLI Surface and Output Contract

- [x] 1.1 Add `--zip` option parsing for `collect` as a boolean mode switch; destination is still controlled by the positional `output` argument (default `.`).
- [x] 1.2 Add `--zip` option parsing for `process` as optional destination semantics (`Option<Path>`, default `.` when flag used without explicit path) and define archive target using standard diagnostic naming.
- [x] 1.3 Reuse existing diagnostic base-name generation for zip mode and append `.zip` for both commands.

## 2. Zip Writer Integration

- [x] 2.1 Add or extend output writer abstraction to support zip-backed writes for API result files.
- [x] 2.2 Implement direct-entry writes to archive during `collect --zip` without a temporary full output directory pass.
- [x] 2.3 Implement zip output flow for `process --zip` that places all API outputs into one archive.
- [x] 2.4 Ensure archive entry paths preserve existing relative file naming conventions.

## 3. Error Handling and Concurrency Safety

- [x] 3.1 Ensure archive writer finalization/flush occurs exactly once and surfaces actionable errors.
- [x] 3.2 Serialize zip entry writes safely when API fetch/process stages run concurrently.
- [x] 3.3 Add path normalization/sanitization for cross-platform archive entry compatibility.

## 4. Verification

- [x] 4.1 Add/update unit and integration tests for `collect --zip` destination handling and filename format parity.
- [x] 4.2 Add/update tests verifying `process --zip` writes a standard `api-diagnostics-*.zip` archive with expected entries.
- [x] 4.3 Run `cargo clippy` and address any introduced warnings.
- [x] 4.4 Run `cargo test` and confirm zip-mode scenarios pass.
